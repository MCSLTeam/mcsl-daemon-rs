use crate::app::AppResources;
use hyper::service::service_fn;
use log::{error, info, trace};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::Notify;

use hyper::header::{HeaderName, CONNECTION, SEC_WEBSOCKET_ACCEPT, SEC_WEBSOCKET_KEY, UPGRADE};
use hyper::http::HeaderValue;
use hyper::upgrade::Upgraded;

use super::{driver::StopToken, Driver, UniDriverConfig};
use crate::remote::protocols::v1::ws_behavior::WsBehavior;
use crate::user::UsersManager;
use anyhow::anyhow;
use hyper::body::{Bytes, Incoming};
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder;
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use tokio_tungstenite::tungstenite::{handshake::derive_accept_key, protocol::Role};
use tokio_tungstenite::WebSocketStream;

type Body = http_body_util::Full<Bytes>;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WsDriverConfig {
    #[serde(flatten)]
    pub uni_config: UniDriverConfig,
}

pub struct WsDriver {
    resources: AppResources,
    protocol_set: u8,
    stop_notification: Arc<Notify>,
}

pub struct WsDriverBuilder {
    resources: Option<AppResources>,
    protocol_set: Option<u8>,
}

impl WsDriverBuilder {
    pub fn new() -> Self {
        Self {
            resources: None,
            protocol_set: None,
        }
    }
    pub fn with_resources(mut self, resources: AppResources) -> Self {
        self.resources = Some(resources);
        self
    }
    pub fn with_protocol_set(mut self, protocol_set: u8) -> Self {
        self.protocol_set = Some(protocol_set);
        self
    }
    pub fn build(self) -> anyhow::Result<WsDriver> {
        let resources = self.resources.ok_or_else(|| anyhow!("resources not set"))?;
        let protocol_set = self
            .protocol_set
            .ok_or_else(|| anyhow!("protocol_set not set"))?;
        Ok(WsDriver {
            resources,
            protocol_set,
            stop_notification: Arc::new(Notify::new()),
        })
    }
}

#[derive(Debug, Deserialize)]
struct LoginParams {
    usr: String,
    pwd: String,
    expired: Option<String>,
}

fn parse_params<T: DeserializeOwned>(query: Option<&str>) -> anyhow::Result<T> {
    if let Some(q) = query {
        let params: Vec<&str> = q.split('&').collect();
        let mut map = HashMap::new();

        for param in params {
            let idx = param.find('=').unwrap();
            let key = param[..idx].to_string();
            let value = param[idx + 1..].to_string();
            map.insert(key, value);
        }

        let json = serde_json::to_string(&map)?;
        info!("params: {}", json);
        let rv = serde_json::from_str::<T>(json.as_str())?;
        return Ok(rv);
    }

    Err(anyhow!("empty query"))
}

async fn login_handler(
    app_resources: AppResources,
    req: Request<Incoming>,
    remote_addr: SocketAddr,
) -> Result<Response<Body>, Infallible> {
    let uri = req.uri();
    let query = uri.query();

    let params = parse_params::<LoginParams>(query);

    if params.is_err() {
        return Ok(Response::builder()
            .status(StatusCode::BAD_REQUEST)
            .body(Body::from("Invalid query"))
            .unwrap());
    }
    let params = params.unwrap();

    let expired = params
        .expired
        .map(|s| s.parse::<u64>().unwrap())
        .unwrap_or(30);
    match app_resources.users.auth(&params.usr, &params.pwd).await {
        Some(_) => match app_resources.users.gen_token(&params.usr, expired).await {
            Ok(token) => Ok(Response::new(Body::from(token))),
            Err(e) => Ok(Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from(e.to_string()))
                .unwrap()),
        },
        None => {
            let response = "Unauthorized";
            Ok(Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .body(Body::from(response))
                .unwrap())
        }
    }
}

async fn handle_ws_connection(
    app_resources: AppResources,
    ws: WebSocketStream<TokioIo<Upgraded>>,
    addr: SocketAddr,
) {
    if let Err(e) = WsBehavior::start(ws, app_resources, addr).await {
        error!("Error handling WebSocket connection: {}", e);
    }
}

async fn ws_handler(
    app_resources: AppResources,
    mut req: Request<Incoming>,
    remote_addr: SocketAddr,
) -> Result<Response<Body>, Infallible> {
    let uri = req.uri();
    let query = uri.query();
    let headers = req.headers();

    let derived = headers
        .get(SEC_WEBSOCKET_KEY)
        .map(|k| derive_accept_key(k.as_bytes()));
    let ver = req.version();

    let token = query.and_then(|q| {
        let params: Vec<&str> = q.split('&').collect();
        params.into_iter().find_map(|param| {
            let parts: Vec<&str> = param.split('=').collect();
            if parts.len() == 2 && parts[0] == "token" {
                Some(parts[1])
            } else {
                None
            }
        })
    });

    let user = if let Some(token) = token {
        app_resources.users.auth_token(token).await
    } else {
        None
    };

    if user.is_none() {
        return Ok(Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .body(Body::from("Unauthorized"))
            .unwrap());
    }
    let res = app_resources.clone();
    let handler = tokio::spawn(async move {
        match hyper::upgrade::on(&mut req).await {
            Ok(upgrade) => {
                let upgraded = TokioIo::new(upgrade);
                handle_ws_connection(
                    res,
                    WebSocketStream::from_raw_socket(upgraded, Role::Server, None).await,
                    remote_addr,
                )
                .await;
            }
            Err(e) => {
                println!("Error upgrading connection: {}", e);
            }
        }
    });
    app_resources.ws_handlers.lock().await.push(handler);

    // send upgrade response
    let mut res = Response::new(Body::default());
    *res.status_mut() = StatusCode::SWITCHING_PROTOCOLS;
    *res.version_mut() = ver;
    res.headers_mut()
        .append(CONNECTION, HeaderValue::from_static("Upgrade"));
    res.headers_mut()
        .append(UPGRADE, HeaderValue::from_static("websocket"));
    res.headers_mut()
        .append(SEC_WEBSOCKET_ACCEPT, derived.unwrap().parse().unwrap());
    Ok(res)
}

async fn handle_request(
    app_resources: AppResources,
    req: Request<Incoming>,
    remote_addr: SocketAddr,
) -> Result<Response<Body>, Infallible> {
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/api/v1") => ws_handler(app_resources, req, remote_addr).await,
        (&Method::POST, "/login") => login_handler(app_resources, req, remote_addr).await,
        (&Method::HEAD, _) => {
            let mut resp = Response::new(Body::default());
            resp.headers_mut().append(
                HeaderName::from_static("x-application"),
                HeaderValue::from_static("mcsl_daemon_rs"),
            );
            Ok(resp)
        }
        _ => {
            // Return 404 not found response.
            Ok(Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("Not Found"))
                .unwrap())
        }
    }
}

#[async_trait::async_trait]
impl Driver for WsDriver {
    async fn run(&self) -> () {
        let uni_cfg = &self
            .resources
            .app_config
            .drivers
            .websocket_driver_config
            .uni_config;
        let addr = SocketAddr::new(uni_cfg.host, uni_cfg.port);

        let listener = TcpListener::bind(&addr).await.expect("bind failed");
        info!("Listening on {}", &addr);
        let builder = Builder::new(TokioExecutor::new());

        let mut http_handlers = vec![];

        let stop_notify = self.stop_notification.clone();
        let cancel_token = self.resources.cancel_token.clone();

        loop {
            tokio::select! {
                conn = listener.accept() => {

                    let (stream, peer_addr) = match conn {
                        Ok(conn) => conn,
                        Err(e) => {
                            info!("accept error: {}", e);
                            tokio::time::sleep(Duration::from_secs(1)).await;
                            continue;
                        }
                    };
                    info!("incoming connection accepted: {}", peer_addr);
                    let io = TokioIo::new(stream);
                    let app_res = self.resources.clone();

                    let cancel_token4http = self.resources.cancel_token.clone();

                    let conn = builder.serve_connection_with_upgrades(
                        io,
                        service_fn(move |req| handle_request(app_res.to_owned(), req, peer_addr))
                    ).into_owned();

                    http_handlers.push(tokio::spawn(async move {
                        tokio::select! {
                            res = conn => {
                                if let Err(err) = res {
                                    error!("connection error: {}", err);
                                }
                            },

                            _ = cancel_token4http.notified() => {
                                info!("http shutting down");
                                return;
                            }
                        }

                        trace!("connection dropped: {}", peer_addr);
                    }));
                },

                _ = stop_notify.notified() => {
                    cancel_token.notify_one();
                    info!("Stop signal received, stop listening and starting shutdown...");
                        break;
                }
            }
        }
        for handler in http_handlers {
            handler.await.unwrap();
        }
        trace!("all http handlers finished");

        let mut ws_handlers = self.resources.ws_handlers.lock().await;
        for handler in ws_handlers.drain(..) {
            handler.await.unwrap();
        }
        trace!("all ws handlers finished");
    }

    fn stop_token(&self) -> StopToken {
        self.stop_notification.clone()
    }

    fn set_protocol_set(&mut self, set: u8) {
        todo!()
    }
    fn protocol_set(&self) -> u8 {
        todo!()
    }
    fn get_driver_type(&self) -> &'static str {
        todo!()
    }
}

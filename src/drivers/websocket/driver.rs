use crate::app::AppResources;
use crate::drivers::Drivers;
use hyper::service::service_fn;
use log::{debug, error, info};
use serde::Deserialize;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::Notify;

use hyper::header::{HeaderName, CONNECTION, SEC_WEBSOCKET_ACCEPT, SEC_WEBSOCKET_KEY, UPGRADE};
use hyper::http::HeaderValue;
use hyper::upgrade::Upgraded;

use super::super::{driver::StopToken, Driver};
use super::ws_behavior::WsBehavior;
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

pub struct WsDriver {
    resources: AppResources,
    stop_notification: Arc<Notify>,
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
        debug!("{} login failed: invalid query", remote_addr);
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
            Ok(token) => {
                debug!(
                    "{} login succeeded with username: {}",
                    remote_addr, params.usr
                );
                Ok(Response::new(Body::from(token)))
            }
            Err(e) => {
                debug!("{} login failed: internal server error.", remote_addr);
                error!("error occurred when user login: {}", e);
                Ok(Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(Body::from(e.to_string()))
                    .unwrap())
            }
        },
        None => {
            let response = "Unauthorized";
            debug!("{} login failed: unauthorized.", remote_addr);
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
        error!("Error occurred when handling WebSocket connection: {}", e);
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
    /// run() |> handle_request() |> GET  |> ws_handler()    |> auth? |> Y |> handle_ws_connection() |> WsBehavior::start()
    ///                           |> POST |> login_handler()
    ///                           |> HEAD
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

                        debug!("connection dropped: {}", peer_addr);
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
        debug!("all http handlers finished");

        let mut ws_handlers = self.resources.ws_handlers.lock().await;
        for handler in ws_handlers.drain(..) {
            handler.await.unwrap();
        }
        debug!("all ws handlers finished");
    }

    fn stop_token(&self) -> StopToken {
        self.stop_notification.clone()
    }

    fn get_driver_type(&self) -> Drivers {
        Drivers::Websocket
    }
}

impl WsDriver {
    pub fn new(resources: AppResources) -> Self {
        Self {
            resources,
            stop_notification: Arc::new(Notify::new()),
        }
    }
}

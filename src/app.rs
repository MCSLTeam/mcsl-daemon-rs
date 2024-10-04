use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::pin::pin;
use std::sync::Arc;
use std::time::Duration;

use anyhow::anyhow;
use hyper::body::Bytes;
use hyper::header::{HeaderName, CONNECTION, SEC_WEBSOCKET_ACCEPT, SEC_WEBSOCKET_KEY, UPGRADE};
use hyper::http::HeaderValue;
use hyper::service::service_fn;
use hyper::upgrade::Upgraded;
use hyper::{body::Incoming as IncomingBody, Method, Request, Response, StatusCode};
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder;
use log::{error, info, trace};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use tokio::sync::{
    watch::{Receiver, Sender},
    Mutex,
};
use tokio::task::JoinHandle;
use tokio::{net::TcpListener, select, sync::watch};
use tokio_tungstenite::tungstenite::{handshake::derive_accept_key, protocol::Role};
use tokio_tungstenite::WebSocketStream;

use crate::remote::ws_behavior::WsBehavior;
use crate::storage::AppConfig;
use crate::user::{JwtClaims, Users, UsersManager};

type Body = http_body_util::Full<Bytes>;

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
    req: Request<IncomingBody>,
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
    mut req: Request<IncomingBody>,
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
        app_resources.users.auth_token(&token).await
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
    req: Request<IncomingBody>,
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

pub struct Resources {
    pub app_config: AppConfig,
    pub users: Users,
    pub cancel_token: Receiver<bool>,
    ws_handlers: Mutex<Vec<JoinHandle<()>>>,
}

async fn init_app_res() -> anyhow::Result<(Resources, Sender<bool>)> {
    let config = AppConfig::new();

    let users = Users::build("users.db").await?;
    users.fix_admin().await?;

    let (tx, rx) = watch::channel(false);
    let resources = Resources {
        app_config: config,
        users,
        ws_handlers: Mutex::new(vec![]),
        cancel_token: rx,
    };
    Ok((resources, tx))
}

pub type AppResources = Arc<Resources>;

pub async fn run_app() -> anyhow::Result<()> {
    let (resources, tx) = init_app_res().await?;

    let addr: SocketAddr = format!("127.0.0.1:{}", &resources.app_config.port)
        .parse()
        .unwrap();
    let listener = TcpListener::bind(&addr).await?;
    info!("Listening on {}", &addr);

    let builder = Builder::new(TokioExecutor::new());
    let mut ctrl_c = pin!(tokio::signal::ctrl_c());

    let mut http_handlers = vec![];

    let app_resources = Arc::new(resources);
    loop {
        select! {
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
                let app_res = app_resources.clone();

                let mut cancel_token4http = app_resources.cancel_token.clone();

                let conn = builder.serve_connection_with_upgrades(
                    io,
                    service_fn(move |req| handle_request(app_res.to_owned(), req, peer_addr))
                ).into_owned();

                http_handlers.push(tokio::spawn(async move {
                    select! {
                        res = conn => {
                            if let Err(err) = res {
                                error!("connection error: {}", err);
                            }
                        },

                        _ = cancel_token4http.changed() => {
                            info!("http shutting down");
                            return;
                        }
                    }

                    trace!("connection dropped: {}", peer_addr);
                }));
            },

            _ = ctrl_c.as_mut() => {
                tx.send(true).unwrap();
                info!("Ctrl-C received,stop listening and starting shutdown...");
                    break;
            }
        }
    }

    for handler in http_handlers {
        handler.await?;
    }
    trace!("all http handlers finished");

    let mut ws_handlers = app_resources.ws_handlers.lock().await;
    for handler in ws_handlers.drain(..) {
        handler.await?;
    }
    trace!("all ws handlers finished");
    Ok(())
}

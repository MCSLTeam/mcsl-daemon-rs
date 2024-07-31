use std::collections::HashMap;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::pin::pin;
use std::sync::Arc;
use std::time::Duration;

use anyhow::anyhow;
use hyper::{body::Incoming as IncomingBody, Method, Request, Response, StatusCode};
use hyper::body::Bytes;
use hyper::header::{CONNECTION, SEC_WEBSOCKET_ACCEPT, SEC_WEBSOCKET_KEY, UPGRADE};
use hyper::http::HeaderValue;
use hyper::service::service_fn;
use hyper::upgrade::Upgraded;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder;
use log::{error, info, trace};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use tokio::{net::TcpListener, sync::watch};
use tokio::sync::watch::Receiver;
use tokio_tungstenite::WebSocketStream;
use tokio_tungstenite::tungstenite::handshake::derive_accept_key;
use tokio_tungstenite::tungstenite::protocol::Role;
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


#[derive(Clone)]
pub struct AppState {
    users: Arc<Users>,
    config: AppConfig,
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
    state: AppState,
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


    let expired = params.expired.map(|s| s.parse::<u64>().unwrap()).unwrap_or(30);
    match state.users.authenticate(&params.usr, &params.pwd) {
        Some(_) => {
            let jwt_claims = JwtClaims::new(
                params.usr.to_string(),
                params.pwd.to_string(),
                expired,
            );
            let token = jwt_claims.to_token(&state.config.secret);
            Ok(Response::new(Body::from(token)))
        }
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
    state: AppState,
    ws: WebSocketStream<TokioIo<Upgraded>>,
    addr: SocketAddr,
    cancel_token: Receiver<bool>,
) {
    if let Err(e) = WsBehavior::start(ws, state, cancel_token,addr).await {
        error!("Error handling WebSocket connection: {}", e);
    }
}

async fn ws_handler(
    state: AppState,
    mut req: Request<IncomingBody>,
    remote_addr: SocketAddr,
    cancel_token: Receiver<bool>,
) -> Result<Response<Body>, Infallible> {
    let uri = req.uri();
    let query = uri.query();
    let headers = req.headers();

    let derived = headers.get(SEC_WEBSOCKET_KEY).map(|k| derive_accept_key(k.as_bytes()));
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

    let mut user_meta = None;
    if let Some(token) = token {
        match JwtClaims::from_token(token, &state.config.secret) {
            Ok(claims) => {
                user_meta = state.users.authenticate(&claims.usr, &claims.pwd);
            }
            Err(e) => trace!("Token validation failed: {}", e)
        }
    }
    if user_meta.is_none() {
        return Ok(Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .body(Body::from("Unauthorized"))
            .unwrap());
    }

    tokio::spawn(async move {
        match hyper::upgrade::on(&mut req).await {
            Ok(upgrade) => {
                let upgraded = TokioIo::new(upgrade);
                handle_ws_connection(
                    state,
                    WebSocketStream::from_raw_socket(upgraded, Role::Server, None).await,
                    remote_addr,
                    cancel_token,
                ).await;
            }
            Err(e) => {
                println!("Error upgrading connection: {}", e);
            }
        }
    });

    // send upgrade response
    let mut res = Response::new(Body::default());
    *res.status_mut() = StatusCode::SWITCHING_PROTOCOLS;
    *res.version_mut() = ver;
    res.headers_mut().append(CONNECTION, HeaderValue::from_static("Upgrade"));
    res.headers_mut().append(UPGRADE, HeaderValue::from_static("websocket"));
    res.headers_mut().append(SEC_WEBSOCKET_ACCEPT, derived.unwrap().parse().unwrap());
    Ok(res)
}

async fn handle_request(
    app_state: AppState,
    req: Request<IncomingBody>,
    remote_addr: SocketAddr,
    cancel_token: Receiver<bool>,
) -> Result<Response<Body>, Infallible> {
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/api/v1") => ws_handler(app_state, req, remote_addr, cancel_token).await,
        (&Method::GET, "/login") => login_handler(app_state, req, remote_addr).await,
        _ => {
            // Return 404 not found response.
            Ok(Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("Not Found"))
                .unwrap())
        }
    }
}

pub async fn run_app() -> anyhow::Result<()> {
    let users = Users::new("users.json");
    users.fix_admin().unwrap();

    let config = AppConfig::new();

    let addr: SocketAddr = format!("127.0.0.1:{}", &config.port).parse().unwrap();
    let listener = TcpListener::bind(&addr).await?;
    info!("Listening on {}", &addr);

    let builder = Builder::new(TokioExecutor::new());
    let mut ctrl_c = pin!(tokio::signal::ctrl_c());

    let app_state = AppState {
        users: Arc::new(users),
        config: config.clone(),
    };

    let (tx, rx) = watch::channel(false);

    let mut handlers = vec![];

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
                let state = app_state.clone();

                let rx_clone = rx.clone();

                let conn = builder.serve_connection_with_upgrades(
                    io,
                    service_fn(move |req| handle_request(state.to_owned(), req, peer_addr,rx_clone.to_owned()))
                ).into_owned();

                handlers.push(tokio::spawn(async move {
                    if let Err(err) = conn.await {
                        error!("connection error: {}", err);
                    }
                    trace!("connection dropped: {}", peer_addr);
                }));
            },

            _ = ctrl_c.as_mut() => {
                tx.send(true).unwrap();
                info!("Ctrl-C received,stop listening and starting shutdown");
                    break;
            }
        }
    }

    for handler in handlers {
        handler.await.unwrap();
    }
    Ok(())
}

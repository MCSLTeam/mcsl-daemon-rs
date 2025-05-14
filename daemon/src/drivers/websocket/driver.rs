use crate::app::AppState;
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
use super::connection::WebsocketConnection;
use anyhow::anyhow;
use hyper::body::{Bytes, Incoming};
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder;
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use tokio_tungstenite::tungstenite::{handshake::derive_accept_key, protocol::Role};
use tokio_tungstenite::WebSocketStream;
use crate::config::AppConfig;

type Body = http_body_util::Full<Bytes>;

pub struct WsDriver {
    resources: AppState,
    stop_notification: Arc<Notify>,
}

async fn subtoken_handler(
    app_state: AppState,
    req: Request<Incoming>,
    remote_addr: SocketAddr,
) -> Result<Response<Body>, Infallible> {
    // TODO
    Ok(Response::builder()
        .status(StatusCode::UNAUTHORIZED)
        .body(Body::from("Unauthorized"))
        .unwrap())
}

async fn handle_ws_connection(
    app_state: AppState,
    ws: WebSocketStream<TokioIo<Upgraded>>,
    addr: SocketAddr,
) {
    let app_state_clone = app_state.clone();
    if let Err(e) = app_state
        .ws_conn_manager
        .serve_connection(ws, app_state_clone, addr)
        .await
    {
        error!("Error occurred when handling WebSocket connection: {}", e);
    }
}

async fn websocket_upgrade_handler(
    app_state: AppState,
    mut req: Request<Incoming>,
    remote_addr: SocketAddr,
) -> Result<Response<Body>, Infallible> {
    let headers = req.headers();
    let derived = headers
        .get(SEC_WEBSOCKET_KEY)
        .map(|k| derive_accept_key(k.as_bytes()));
    let ver = req.version();

    // verify connection
    if let Err(err_resp) = WebsocketConnection::verify_connection(app_state.clone(), &req, &remote_addr).await
    {
        return Ok(err_resp);
    }

    let state = app_state.clone();
    let handler = tokio::spawn(async move {
        match hyper::upgrade::on(&mut req).await {
            Ok(upgrade) => {
                let upgraded = TokioIo::new(upgrade);
                handle_ws_connection(
                    state,
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
    app_state.ws_connections.lock().await.push(handler);

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

async fn http_request_handler(
    app_state: AppState,
    req: Request<Incoming>,
    remote_addr: SocketAddr,
) -> Result<Response<Body>, Infallible> {
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/api/v1") => websocket_upgrade_handler(app_state, req, remote_addr).await,
        (&Method::POST, "/subtoken") => subtoken_handler(app_state, req, remote_addr).await,
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
        let uni_cfg = &AppConfig::get()
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
                        service_fn(move |req| http_request_handler(app_res.to_owned(), req, peer_addr))
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
        debug!("all http connection closed");

        let mut ws_handlers = self.resources.ws_connections.lock().await;
        for handler in ws_handlers.drain(..) {
            handler.await.unwrap();
        }
        debug!("all ws connection closed");
    }

    fn stop_token(&self) -> StopToken {
        self.stop_notification.clone()
    }

    fn get_driver_type(&self) -> Drivers {
        Drivers::Websocket
    }
}

impl WsDriver {
    pub fn new(resources: AppState) -> Self {
        Self {
            resources,
            stop_notification: Arc::new(Notify::new()),
        }
    }
}

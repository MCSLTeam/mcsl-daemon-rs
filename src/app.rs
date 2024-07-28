use std::net::SocketAddr;
use std::sync::Arc;

use axum::{http, Router};
use axum::extract::{ConnectInfo, FromRef, Query, State};
use axum::http::Response;
use axum::response::IntoResponse;
use axum::routing::get;
use fastwebsockets::{OpCode, upgrade, WebSocketError};
use http_body_util::Empty;
use log::{info, trace, warn};
use serde::Deserialize;
use tokio::net::TcpListener;
use tokio::signal;

use crate::storage::AppConfig;
use crate::user::{JwtClaims, Users, UsersManager};
use crate::user::users::UserMeta;

#[derive(Debug, Deserialize)]
struct LoginParams {
    usr: String,
    pwd: String,
    expired: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct WsParams {
    token: String,
}

impl FromRef<AppState> for AppConfig {
    fn from_ref(state: &AppState) -> Self {
        state.config.clone()
    }
}

async fn login_handler(
    Query(params): Query<LoginParams>,
    State(app_state): State<Arc<AppState>>,
) -> impl IntoResponse {
    let expired = params.expired.unwrap_or(30);
    match app_state.users.authenticate(&params.usr, &params.pwd) {
        Some(_) => {
            let jwt_claims = JwtClaims::new(
                params.usr.to_string(),
                params.pwd.to_string(),
                expired,
            );

            (http::StatusCode::OK, jwt_claims.to_token(&app_state.config.secret))
        }
        None => {
            let response = "Unauthorized";
            (
                http::StatusCode::UNAUTHORIZED,
                response.to_string(),
            )
        }
    }
}

async fn handle_client(fut: upgrade::UpgradeFut) -> Result<(), WebSocketError> {
    let mut ws = fastwebsockets::FragmentCollector::new(fut.await?);
    let mut fragment_buffer = vec![];
    let mut last_opcode = OpCode::Continuation;
    loop {
        let frame = ws.read_frame().await?;
        match &frame.opcode {
            OpCode::Close => break,
            OpCode::Continuation => {
                if !frame.fin {
                    fragment_buffer.extend(frame.payload.to_vec());
                } else {
                    fragment_buffer.extend(frame.payload.to_vec());
                    dispatch_frame(last_opcode, fragment_buffer.to_vec()).await;
                    fragment_buffer.clear();
                }
            }
            _ => {
                last_opcode = frame.opcode;
                if frame.fin {
                    dispatch_frame(frame.opcode, frame.payload.to_vec()).await;
                }
            }
        }
    }

    Ok(())
}

async fn dispatch_frame(op_code: OpCode, payload: Vec<u8>) {
    match op_code {
        OpCode::Text => {
            println!("{}", String::from_utf8(payload).unwrap())
        }
        _ => {
            warn!("Unsupported opcode: {:?}", op_code)
        }
    }
}
async fn ws_handler(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Query(params): Query<WsParams>,
    State(app_state): State<Arc<AppState>>,
    ws: upgrade::IncomingUpgrade,
) -> impl IntoResponse {
    info!("Received WebSocket connection from: {}, token={}",addr,params.token);
    let user_meta:UserMeta;
    match JwtClaims::from_token(&params.token, &app_state.config.secret) {
        Ok(claims) => {
            match &app_state.users.authenticate(&claims.usr, &claims.pwd){
                Some(user) => {
                    user_meta = user.clone();
                },
                None => {
                    return Response::builder()
                        .status(http::StatusCode::UNAUTHORIZED)
                        .body(Empty::new())
                        .unwrap()
                }
            }
        }
        Err(e)=>{
            trace!("Token validation failed: {}", &e);
            return Response::builder()
                .status(http::StatusCode::UNAUTHORIZED)
                .body(Empty::new())
                .unwrap()
        }
    }
    let (resp, fut) = ws.upgrade().unwrap();
    tokio::task::spawn(async move {
        if let Err(e) = handle_client(fut).await {
            eprintln!("Error in websocket connection: {}", e);
        }
    });
    trace!("{:?}",&resp);
    resp
}

// 返回一个 future，当接收到 Ctrl+C 或者其他关闭信号时完成
async fn shutdown_signal() {

    // 用于接收 Ctrl+C 信号
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("Failed to install Ctrl+C handler");
    };

    // 用于接收 SIGTERM 信号
    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    println!("App shutting down...");
}

struct AppState {
    users: Arc<Users>,
    config: AppConfig,
}
pub struct App {
    users: Arc<Users>,
    config: AppConfig,
}

impl App {
    pub fn new() -> Self {
        let users = Users::new("users.json");
        let config = AppConfig::new();
        users.fix_admin().unwrap();
        App {
            users: Arc::new(users),
            config,
        }
    }


    pub async fn start(&self) -> anyhow::Result<()> {
        let listener = TcpListener::bind(format!("127.0.0.1:{}", &self.config.port)).await?;
        info!("App started at http://127.0.0.1:{}", &self.config.port);

        let app_state = Arc::new(AppState {
            users: self.users.clone(),
            config: self.config.clone(),
        });

        let app = Router::new()
            .route("/login", get(login_handler))
            .route("/api/v1", get(ws_handler))
            .with_state(app_state);

        axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
            .with_graceful_shutdown(shutdown_signal())
            .await?;
        Ok(())
    }
}
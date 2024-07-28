use std::sync::Arc;

use axum::extract::{FromRef, Query, State};
use axum::response::IntoResponse;
use axum::Router;
use axum::routing::get;
use jsonwebtoken::{encode, EncodingKey, Header};
use log::info;
use serde::Deserialize;
use tokio::net::TcpListener;
use fastwebsockets::{OpCode, upgrade, WebSocketError};
use tokio::signal;
use crate::storage::AppConfig;
use crate::user;
use crate::user::{Users, UsersManager};

#[derive(Debug, Deserialize)]
struct LoginParams {
    usr: String,
    pwd: String,
    expired: Option<u64>,
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
    return match app_state.users.authenticate(&params.usr, &params.pwd) {
        Some(user) => {
            let jwt_claims = user::JwtClaims::new(
                params.usr.to_string(),
                user.pwd_hash.to_string(),
                expired,
            );
            let token = encode(
                &Header::default(),
                &jwt_claims,
                &EncodingKey::from_secret(app_state.config.secret.as_bytes()),
            ).unwrap();

            (axum::http::StatusCode::OK, token)
        }
        None => {
            let response = "Unauthorized";
            (
                axum::http::StatusCode::UNAUTHORIZED,
                response.to_string(),
            )
        }
    };
}

async fn handle_client(fut: upgrade::UpgradeFut) -> Result<(), WebSocketError> {
    let mut ws = fastwebsockets::FragmentCollector::new(fut.await?);

    loop {
        let frame = ws.read_frame().await?;
        match frame.opcode {
            OpCode::Close => break,
            OpCode::Text | OpCode::Binary => {
                ws.write_frame(frame).await?;
            }
            _ => {}
        }
    }

    Ok(())
}
async fn ws_handler(
    State(_app_state): State<Arc<AppState>>,
    ws: upgrade::IncomingUpgrade,
) -> impl IntoResponse {
    let (resp, fut) = ws.upgrade().unwrap();
    tokio::task::spawn(async move {
        if let Err(e) = handle_client(fut).await {
            eprintln!("Error in websocket connection: {}", e);
        }
    });
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

        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown_signal())
            .await?;
        Ok(())
    }
}
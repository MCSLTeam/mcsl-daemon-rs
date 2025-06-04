use crate::auth::{JwtClaims, JwtCodec};
use crate::drivers::websocket::WebsocketConnection;
use crate::drivers::Driver;
use crate::{app::AppState, config::AppConfig, drivers::Drivers};
use axum::extract::Query;
use axum::http::header;
use axum::{
    body::Body,
    extract::{
        multipart::Multipart,
        ws::{WebSocket, WebSocketUpgrade},
        ConnectInfo, State,
    },
    http::{HeaderMap, Method, Response, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use log::{debug, error, info};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::net::SocketAddr;
use thiserror::Error;
use tokio::net::TcpListener;
use tower_http::cors::CorsLayer;

pub struct WsDriver {
    app_state: AppState,
}

#[derive(Deserialize)]
struct SubtokenForm {
    pub token: String,
    pub permissions: String,
    pub expires: Option<u64>,
}

#[async_trait::async_trait]
impl Driver for WsDriver {
    async fn run(&self) {
        let uni_cfg = &AppConfig::get().drivers.websocket_driver_config.uni_config;
        let addr = SocketAddr::new(uni_cfg.host, uni_cfg.port);

        let app = Router::new()
            .route("/api/v1", get(ws_handler))
            .route("/subtoken", post(subtoken_handler))
            .route("/info", get(info_handler))
            .with_state(self.app_state.clone())
            .layer(
                CorsLayer::new()
                    .allow_origin(tower_http::cors::Any)
                    .allow_methods([Method::GET, Method::POST]),
            )
            .into_make_service_with_connect_info::<SocketAddr>();

        let listener = TcpListener::bind(addr).await.expect("Failed to bind");
        info!("WebSocket server listening on {}", addr);

        let stop_token = self.app_state.stop_notify.clone();
        let state = self.app_state.clone();
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                stop_token.notified().await;
                info!("Shutdown signal received, closing connections...");

                let mut ws_handlers = state.ws_connections.lock().await;
                for handler in ws_handlers.drain(..) {
                    if let Err(err) = handler.await {
                        error!("Error handling websocket connection: {}", err);
                    }
                }
            })
            .await
            .unwrap();
    }

    fn get_driver_type(&self) -> Drivers {
        Drivers::Websocket
    }
}

// WebSocket处理函数
async fn ws_handler(
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Query(params): Query<HashMap<String, String>>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    info!("WebSocket connection received from {:?}", addr);
    // 执行验证逻辑
    match WebsocketConnection::verify_connection(state.clone(), &headers, params, &addr).await {
        Ok(claims) => {
            ws.on_upgrade(move |socket| handle_ws_connection(socket, claims, state, addr))
        }
        Err(reason) => Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .body(reason.into())
            .unwrap(),
    }
}

// WebSocket连接处理
async fn handle_ws_connection(
    socket: WebSocket,
    claims: JwtClaims,
    state: AppState,
    addr: SocketAddr,
) {
    let state_clone = state.clone();

    // 将连接加入管理
    let join_handle = tokio::spawn(async move {
        let state_clone = state.clone();
        match state
            .ws_conn_manager
            .serve_connection(socket, claims, state_clone, addr)
            .await
        {
            Ok(_) => debug!("WebSocket connection closed: {}", addr),
            Err(e) => error!("WebSocket error: {}: {}", addr, e),
        }
    });

    state_clone.ws_connections.lock().await.push(join_handle);
}

#[derive(Debug, Error)]
enum HandlerError {
    #[error("Invalid field: {0}")]
    FieldError(String),
    #[error("Invalid expiration time")]
    InvalidExpires,
    #[error("Unauthorized")]
    Unauthorized,
}

impl IntoResponse for HandlerError {
    fn into_response(self) -> Response<Body> {
        let status = match self {
            HandlerError::Unauthorized => StatusCode::UNAUTHORIZED,
            _ => StatusCode::BAD_REQUEST,
        };

        Response::builder()
            .status(status)
            .header(header::CONTENT_TYPE, "text/plain")
            .body(Body::from(self.to_string()))
            .unwrap()
    }
}

async fn subtoken_handler(mut multipart: Multipart) -> Result<Response<Body>, HandlerError> {
    let mut token = None;
    let mut permissions = None;
    let mut expires = None;

    // 处理 multipart 字段
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| HandlerError::FieldError(e.to_string()))?
    {
        let field_name = field
            .name()
            .ok_or(HandlerError::FieldError("Missing field name".into()))?;

        match field_name {
            "token" => {
                token =
                    Some(field.text().await.map_err(|e| {
                        HandlerError::FieldError(format!("Token field error: {}", e))
                    })?);
            }
            "permissions" => {
                permissions = Some(field.text().await.map_err(|e| {
                    HandlerError::FieldError(format!("Permissions field error: {}", e))
                })?);
            }
            "expires" => {
                let expires_str = field
                    .text()
                    .await
                    .map_err(|e| HandlerError::FieldError(format!("Expires field error: {}", e)))?;

                expires = if !expires_str.is_empty() {
                    Some(
                        expires_str
                            .parse::<i64>()
                            .map_err(|_| HandlerError::InvalidExpires)?,
                    )
                } else {
                    None
                };
            }
            _ => {
                return Err(HandlerError::FieldError(format!(
                    "Unknown field: {}",
                    field_name
                )))
            }
        }
    }

    // 验证必需字段
    let token = token.ok_or(HandlerError::FieldError("Missing token".into()))?;
    let permissions = permissions.ok_or(HandlerError::FieldError("Missing permissions".into()))?;

    // 验证主令牌
    if !AppConfig::get().auth.main_token.eq(&token) {
        return Err(HandlerError::Unauthorized);
    }

    // 生成 JWT
    let expires_seconds = expires.unwrap_or(30);
    let claims = JwtClaims::new(expires_seconds, permissions);
    let jwt = claims.to_token();

    // 构建响应
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/plain")
        .body(Body::from(jwt))
        .unwrap())
}

// info请求处理
async fn info_handler() -> impl IntoResponse {
    // 构建 JSON 响应内容
    let response_body = json!({
        "name": "MCServerLauncher Future Daemon Rust",
        "version": crate::app::VERSION,
        "api_version": "v1"
    })
    .to_string();

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(response_body))
        .unwrap()
}

impl WsDriver {
    pub fn new(app_state: AppState) -> Self {
        Self { app_state }
    }
}

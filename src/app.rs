use std::sync::Arc;

use axum::extract::{FromRef, Query, State};
use axum::response::IntoResponse;
use axum::Router;
use axum::routing::get;
use jsonwebtoken::{encode, EncodingKey, Header};
use log::info;
use serde::Deserialize;
use tokio::net::TcpListener;

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
        info!("Server started at http://127.0.0.1:{}", &self.config.port);

        let app_state = Arc::new(AppState {
            users: self.users.clone(),
            config: self.config.clone(),
        });

        let app = Router::new()
            .route("/login", get(login_handler))
            .with_state(app_state);
        axum::serve(listener, app).await?;
        Ok(())
    }
}
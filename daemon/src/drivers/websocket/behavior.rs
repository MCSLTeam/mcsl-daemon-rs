use crate::app::AppState;
use crate::auth::{JwtClaims, JwtCodec};
use crate::config::AppConfig;
use crate::drivers::websocket::{WebsocketConnection, WebsocketContext};
use crate::protocols::{Protocol, Protocols};
use axum::body::Bytes;
use axum::extract::ws::{CloseFrame, Message, Utf8Bytes};
use axum::http::HeaderMap;
use log::info;
use std::collections::HashMap;
use std::net::SocketAddr;

impl WebsocketConnection {
    pub fn handle_received(&self, data: Message) -> anyhow::Result<()> {
        match data {
            Message::Text(text) => {
                info!("received text: {}", text);

                let v1 = self.app_state.protocol_v1.clone();
                let sender = self.sender.downgrade();
                let protocols = self.app_state.protocols;

                tokio::spawn(async move {
                    if protocols.is_enabled(Protocols::V1) {
                        if let Some(text) = v1.process_text(text.to_string()).await {
                            Self::weak_send(sender, Message::Text(Utf8Bytes::from(text)));
                        }
                    }
                });
            }
            Message::Binary(bin) => {
                let v1 = self.app_state.protocol_v1.clone();
                let sender = self.sender.downgrade();
                let protocols = self.app_state.protocols;

                tokio::spawn(async move {
                    if protocols.is_enabled(Protocols::V1) {
                        if let Some(bin) = v1.process_binary(bin.to_vec()).await {
                            Self::weak_send(sender, Message::Binary(Bytes::from(bin)));
                        }
                    }
                });
            }
            Message::Close(close) => {
                self.handle_closing(close.as_ref());
            }
            _ => {}
        }
        Ok(())
    }

    pub fn handle_closing(&self, msg: Option<&CloseFrame>) {
        info!(
            "websocket close from client({}), with reason: {}",
            self.addr,
            msg.map(|f| f.reason.clone()).unwrap_or_default()
        );
    }
}

impl WebsocketConnection {
    pub async fn verify_connection(
        _app_state: AppState,
        _req: &HeaderMap,
        query: HashMap<String, String>,
        _remote_addr: &SocketAddr,
    ) -> Result<WebsocketContext, String> {
        let token = query
            .get("token")
            .ok_or_else(|| "Missing required 'token' field: `token`".to_string())?;

        if AppConfig::get().auth.main_token.eq(token.trim()) {
            Ok(WebsocketContext::default())
        } else {
            let claims = JwtClaims::from_token(token).map_err(|err| err.to_string())?;
            WebsocketContext::try_from(claims)
        }
    }
}

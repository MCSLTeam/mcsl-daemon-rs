use crate::app::AppState;
use crate::auth::{JwtClaims, JwtCodec};
use crate::config::AppConfig;
use crate::drivers::websocket::WebsocketConnection;
use crate::protocols::v1::ProtocolV1;
use crate::protocols::{Protocol, Protocols};
use axum::body::Bytes;
use axum::extract::ws::{CloseFrame, Message, Utf8Bytes};
use axum::http::HeaderMap;
use log::info;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::mpsc::error::SendError;

impl WebsocketConnection {
    pub async fn verify_connection(
        _app_state: AppState,
        _req: &HeaderMap,
        query: HashMap<String, String>,
        _remote_addr: &SocketAddr,
    ) -> Result<JwtClaims, String> {
        let token = query
            .get("token")
            .ok_or_else(|| "Missing required 'token' field: `token`".to_string())?;

        if AppConfig::get().auth.main_token.eq(token.trim()) {
            Ok(JwtClaims::default())
        } else {
            JwtClaims::from_token(token).map_err(|err| err.to_string())
        }
    }

    pub async fn handle_received(
        data: Message,
        v1: Arc<ProtocolV1>,
        protocols: Protocols,
        addr: SocketAddr,
    ) -> Option<Message> {
        match data {
            Message::Text(text) => {
                info!("received text: {}", text);

                if protocols.is_enabled(Protocols::V1) {
                    v1.process_text(text.as_ref())
                        .await
                        .map(|text| Message::Text(Utf8Bytes::from(text)))
                } else {
                    None
                }
            }
            Message::Binary(bin) => {
                if protocols.is_enabled(Protocols::V1) {
                    v1.process_binary(bin.as_ref())
                        .await
                        .map(|bin| Message::Binary(Bytes::from(bin)))
                } else {
                    None
                }
            }
            Message::Close(close) => {
                Self::handle_closing(close.as_ref(), &addr);
                None
            }
            _ => None,
        }
    }

    pub fn handle_closing(msg: Option<&CloseFrame>, addr: &SocketAddr) {
        info!(
            "websocket close from client({}), with reason: {}",
            addr,
            msg.map(|f| f.reason.clone()).unwrap_or_default()
        );
    }

    pub fn handle_too_many_requests(
        &self,
        data: Message,
    ) -> Result<(), SendError<Option<Message>>> {
        let msg = match data {
            Message::Text(text) => {
                if self.app_state.protocols.is_enabled(Protocols::V1) {
                    self.app_state
                        .protocol_v1
                        .handle_text_rate_limit_exceed(text.as_ref())
                        .map(|text| Message::Text(Utf8Bytes::from(text)))
                } else {
                    None
                }
            }
            Message::Binary(bin) => {
                if self.app_state.protocols.is_enabled(Protocols::V1) {
                    self.app_state
                        .protocol_v1
                        .handle_bin_rate_limit_exceed(bin.as_ref())
                        .map(|bin| Message::Binary(Bytes::from(bin)))
                } else {
                    None
                }
            }
            _ => None,
        };
        self.sender.send(msg)
    }
}

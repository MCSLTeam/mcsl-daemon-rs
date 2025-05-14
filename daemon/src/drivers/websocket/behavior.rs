use crate::app::AppState;
use crate::drivers::websocket::WebsocketConnection;
use crate::protocols::{Protocol, Protocols};
use hyper::body::{Bytes, Incoming};
use hyper::{Request, Response};
use log::info;
use std::net::SocketAddr;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::Message;
type Body = http_body_util::Full<Bytes>;
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
                        if let Some(text) = v1.process_text(text).await {
                            Self::weak_send(sender, Message::Text(text));
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
                        if let Some(bin) = v1.process_binary(bin).await {
                            Self::weak_send(sender, Message::Binary(bin));
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

    pub fn handle_closing(&self, msg: Option<&CloseFrame<'_>>) {
        info!(
            "websocket close from client({}), with reason: {}",
            self.addr,
            msg.map(|f| f.reason.clone()).unwrap_or_default()
        );
    }
}

impl WebsocketConnection {
    pub async fn verify_connection(
        app_state: AppState,
        req: &Request<Incoming>,
        remote_addr: &SocketAddr,
    ) -> Result<(), Response<Body>> {
        Ok(())
    }
}

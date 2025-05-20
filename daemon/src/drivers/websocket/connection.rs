use axum::extract::ws::{close_code, CloseFrame, Message, WebSocket};
use futures::{SinkExt, StreamExt};
use log::{debug, info};
use std::net::SocketAddr;
use std::ops::Add;
use std::sync::atomic::AtomicUsize;
use std::sync::{atomic, Arc};
use tokio::select;
use tokio::sync::mpsc::WeakUnboundedSender;
use tokio::sync::mpsc::{error::SendError, unbounded_channel, UnboundedSender};

use crate::app::AppState;
use crate::auth::{JwtClaims, Permissions};

pub struct WebsocketContext {
    pub permissions: Permissions,
    pub expire_to: chrono::DateTime<chrono::Utc>,
    pub jti: uuid::Uuid,
}

impl Default for WebsocketContext {
    fn default() -> Self {
        Self {
            permissions: Permissions::always(),
            expire_to: chrono::Utc::now().add(chrono::Duration::days(10000)),
            jti: uuid::Uuid::default(),
        }
    }
}

impl TryFrom<JwtClaims> for WebsocketContext {
    type Error = String;

    fn try_from(value: JwtClaims) -> Result<Self, Self::Error> {
        Ok(Self {
            permissions: Permissions::from_str(&value.perms).map_err(|e| e.to_string())?,
            expire_to: chrono::DateTime::from_timestamp(value.exp as i64, 0).unwrap(),
            jti: uuid::Uuid::parse_str(&value.jti).map_err(|e| e.to_string())?,
        })
    }
}

pub struct WebsocketConnection {
    #[allow(dead_code)]
    pub app_state: AppState,

    pub sender: UnboundedSender<Message>,
    pub addr: SocketAddr,
}

impl WebsocketConnection {
    fn new(
        app_state: AppState,
        sender: UnboundedSender<Message>,
        addr: SocketAddr,
    ) -> WebsocketConnection {
        WebsocketConnection {
            app_state,
            sender,
            addr,
        }
    }
}

impl WebsocketConnection {
    pub fn send(&self, msg: Message) -> Result<(), SendError<Message>> {
        self.sender.clone().send(msg)
    }

    pub fn weak_send(weak_sender: WeakUnboundedSender<Message>, data: Message) {
        if let Some(sender) = weak_sender.upgrade() {
            if let Err(msg) = sender.send(data) {
                debug!("could not send message due to ws sender dropped: {}", msg);
            }
        } else {
            debug!(
                "could not send message due to ws sender dropped: {:#?}",
                data
            );
        }
    }

    pub fn stop(&self) -> anyhow::Result<()> {
        let close_frame = Some(CloseFrame {
            code: 1000,
            reason: "".into(),
        });
        self.handle_closing(close_frame.as_ref());
        self.send(Message::Close(close_frame))?;
        Ok(())
    }
}
pub struct WsConnManager {
    id: AtomicUsize,
    connections: scc::HashMap<usize, Arc<WebsocketConnection>, ahash::RandomState>,
}

unsafe impl Send for WsConnManager {}

impl Default for WsConnManager {
    fn default() -> Self {
        Self::new()
    }
}

impl WsConnManager {
    pub fn new() -> Self {
        Self {
            id: AtomicUsize::new(0),
            connections: scc::HashMap::default(),
        }
    }
}

impl WsConnManager {
    fn add(&self, conn: Arc<WebsocketConnection>) -> usize {
        let id = self.id.fetch_add(1, atomic::Ordering::Relaxed);
        let _ = self.connections.insert(id, conn);
        id
    }

    pub async fn serve_connection(
        &self,
        ws: WebSocket,
        app_state: AppState,
        peer_addr: SocketAddr,
    ) -> anyhow::Result<()> {
        let (mut outgoing, mut incoming) = ws.split();

        let (outgoing_tx, mut outgoing_rx) = unbounded_channel();

        let ws_conn = Arc::new(WebsocketConnection::new(
            app_state.clone(),
            outgoing_tx,
            peer_addr,
        ));

        let cancel_token = app_state.stop_notify.clone();

        let ws_conn_clone = ws_conn.clone();

        let connection_loop = || async move {
            loop {
                select! {
                    // read
                    msg = incoming.next() => {
                        if let Some(Ok(m)) = msg {
                            ws_conn_clone.handle_received(m)?
                        }
                        else {
                            break;
                        }
                    }

                    // write
                    msg = outgoing_rx.recv() => {
                        if let Some(m) = msg {
                             match m {
                                Message::Close(_) => {
                                    outgoing.send(m).await?;
                                    outgoing.close().await?;
                                }
                                _ => outgoing.send(m).await?,
                            }
                        }
                        else {
                            break;
                        }
                    }

                    // cancel
                    _ = cancel_token.notified() => {
                        outgoing.send(Message::Close(Some(CloseFrame{
                            code: close_code::NORMAL,
                            reason: "daemon closed".into()
                        }))).await?;
                        info!("websocket connection from {} closed", peer_addr);
                        break;
                    }
                }
            }
            anyhow::Ok(())
        };
        let id = self.add(ws_conn);
        let rv = tokio::spawn(connection_loop()).await?;
        self.connections.remove(&id);
        rv
    }
}

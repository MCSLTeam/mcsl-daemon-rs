use futures::{SinkExt, StreamExt};
use hyper::upgrade::Upgraded;
use hyper_util::rt::TokioIo;
use log::{debug, info};
use std::net::SocketAddr;
use std::sync::atomic::AtomicUsize;
use std::sync::{atomic, Arc};
use tokio::select;
use tokio::sync::mpsc::WeakUnboundedSender;
use tokio::sync::mpsc::{error::SendError, unbounded_channel, UnboundedSender};
use tokio_tungstenite::tungstenite::{
    protocol::{frame::coding::CloseCode, CloseFrame},
    Message,
};
use tokio_tungstenite::WebSocketStream;

use crate::app::AppState;
use crate::protocols::{Protocol, Protocols};

pub struct WebsocketConnection {
    #[allow(dead_code)]
    pub app_state: AppState,

    pub sender: UnboundedSender<Message>,
    pub addr: SocketAddr,
}

impl WebsocketConnection {
    fn new(
        app_resources: AppState,
        sender: UnboundedSender<Message>,
        addr: SocketAddr,
    ) -> WebsocketConnection {
        WebsocketConnection {
            app_state: app_resources,
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
            debug!("could not send message due to ws sender dropped: {}", data);
        }
    }

    pub fn stop(&self) -> anyhow::Result<()> {
        let close_frame = Some(CloseFrame {
            code: CloseCode::Normal,
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

impl WsConnManager {
    pub fn new() -> Self {
        Self {
            id: AtomicUsize::new(0),
            connections: scc::HashMap::default(),
        }
    }
}

impl WsConnManager {
    fn add(&self, conn: Arc<WebsocketConnection>) {
        let _ = self
            .connections
            .insert(self.id.fetch_add(1, atomic::Ordering::Relaxed), conn);
    }

    pub async fn serve_connection(
        &self,
        ws: WebSocketStream<TokioIo<Upgraded>>,
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

        let cancel_token = app_state.cancel_token.clone();

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
                            info!("connection read loop ended");
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
                            info!("connection write loop ended");
                            break;
                        }
                    }

                    // cancel
                    _ = cancel_token.notified() => {
                        // ws_conn_clone.stop()?;
                        outgoing.send(Message::Close(Some(CloseFrame{
                            code: CloseCode::Normal,
                            reason: "daemon closed".into()
                        }))).await?;
                        info!("websocket connection from {} closed", peer_addr);
                        break;
                    }
                }
            }
            anyhow::Ok(())
        };

        let rv = tokio::spawn(connection_loop()).await?;
        info!("connection finished");
        rv
    }
}

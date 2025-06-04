use crate::app::AppState;
use crate::auth::{JwtClaims, Permissions};
use crate::config::AppConfig;
use crate::utils::task_pool::TaskPool;
use anyhow::Context;
use axum::extract::ws::{close_code, CloseFrame, Message, WebSocket};
use futures::{SinkExt, StreamExt};
use log::info;
use std::net::SocketAddr;
use std::sync::atomic::AtomicUsize;
use std::sync::{atomic, Arc};
use tokio::select;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::mpsc::{error::SendError, unbounded_channel, UnboundedSender};
use tokio::sync::Notify;

pub struct WebsocketContext {
    pub permissions: Permissions,
    pub expire_to: chrono::DateTime<chrono::Utc>,
    pub jti: uuid::Uuid,
    pub peer_addr: SocketAddr,
    pub connection_id: usize,
}

impl WebsocketContext {
    pub fn new(
        claims: JwtClaims,
        peer_addr: SocketAddr,
        connection_id: usize,
    ) -> anyhow::Result<Self> {
        Ok(Self {
            permissions: Permissions::from_str(&claims.perms).context("invalid permissions")?,
            expire_to: chrono::DateTime::from_timestamp(claims.exp, 0).unwrap(),
            jti: uuid::Uuid::parse_str(&claims.jti).context("invalid jti")?,
            peer_addr,
            connection_id,
        })
    }
}

pub struct WebsocketConnection {
    #[allow(dead_code)]
    pub app_state: AppState,
    pub context: WebsocketContext,
    pub sender: UnboundedSender<Option<Message>>,
    pub addr: SocketAddr,
    task_pool: TaskPool<Message, Option<Message>>,
}

impl WebsocketConnection {
    fn new(
        app_state: AppState,
        context: WebsocketContext,
        sender: UnboundedSender<Option<Message>>,
        addr: SocketAddr,
        task_pool: TaskPool<Message, Option<Message>>,
    ) -> WebsocketConnection {
        WebsocketConnection {
            app_state,
            context,
            sender,
            addr,
            task_pool,
        }
    }
}

impl WebsocketConnection {
    pub fn send(&self, msg: Message) -> Result<(), SendError<Message>> {
        self.sender
            .send(Some(msg))
            .map_err(|err| SendError(err.0.unwrap()))
    }

    pub fn stop(&self) -> anyhow::Result<()> {
        let close_frame = Some(CloseFrame {
            code: 1000,
            reason: "".into(),
        });
        Self::handle_closing(close_frame.as_ref(), &self.addr);
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
    pub async fn serve_connection(
        &self,
        ws: WebSocket,
        claims: JwtClaims,
        app_state: AppState,
        peer_addr: SocketAddr,
    ) -> anyhow::Result<()> {
        let (outgoing_tx, outgoing_rx) = unbounded_channel();
        let id = self.id.fetch_add(1, atomic::Ordering::Relaxed);
        let pool = TaskPool::new(
            {
                let v1 = app_state.protocol_v1.clone();
                let protocols = app_state.protocols;
                let addr = peer_addr;
                move |data: Message| {
                    let v1 = v1.clone();
                    Box::pin(WebsocketConnection::handle_received(
                        data, v1, protocols, addr,
                    ))
                }
            },
            AppConfig::get().protocols.v1.max_parallel_requests as usize,
            AppConfig::get().protocols.v1.max_pending_requests as usize,
            outgoing_tx.clone(),
            60,
        );
        let ws_conn = Arc::new(WebsocketConnection::new(
            app_state.clone(),
            WebsocketContext::new(claims, peer_addr, id)
                .context("could not create WebsocketContext")?,
            outgoing_tx,
            peer_addr,
            pool,
        ));
        let _ = self.connections.insert(id, ws_conn.clone());

        self.connection_loop(ws, app_state.stop_notify.clone(), outgoing_rx, ws_conn)
            .await
            .context("error occurred while serving connection")?;

        self.connections.remove(&id);
        Ok(())
    }

    async fn connection_loop(
        &self,
        ws: WebSocket,
        cancel_token: Arc<Notify>,
        mut outgoing_rx: UnboundedReceiver<Option<Message>>,
        conn: Arc<WebsocketConnection>,
    ) -> anyhow::Result<()> {
        let (mut outgoing, mut incoming) = ws.split();

        loop {
            select! {
                // read
                msg = incoming.next() => {
                    if let Some(Ok(m)) = msg {
                        if let Err(err) = conn.task_pool.try_submit(m).await{
                            match err{
                                kanal::TrySendError::Full(m) => {
                                    conn.handle_too_many_requests(m).await?
                                }
                                _ => {break;}
                            }
                        }
                    }
                    else {
                        break;
                    }
                }

                // write
                msg = outgoing_rx.recv() => {
                    match msg {
                        Some(Some(m))=>{
                            match m {
                                Message::Close(_) => {
                                    outgoing.send(m).await?;
                                    outgoing.close().await?;
                                }
                                _ => outgoing.send(m).await?,
                            }
                        }
                        None => {break;}
                        _ => {}
                    }
                }

                // cancel
                _ = cancel_token.notified() => {
                    outgoing.send(Message::Close(Some(CloseFrame{
                        code: close_code::NORMAL,
                        reason: "daemon closed".into()
                    }))).await?;
                    info!("websocket connection from {} closed", &conn.context.peer_addr);
                    break;
                }
            }
        }
        Ok(())
    }
}

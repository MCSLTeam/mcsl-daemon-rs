use std::net::SocketAddr;
use anyhow::anyhow;
use futures::channel::mpsc::{unbounded,  UnboundedSender};
use futures::{SinkExt, StreamExt, TryFutureExt};
use hyper::upgrade::Upgraded;
use hyper_util::rt::TokioIo;
use log::info;
use serde::Serialize;
use serde_json::{json, Value};
use tokio::select;
use tokio::sync::Mutex;
use tokio::sync::watch::Receiver;
use tokio::task::JoinError;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::protocol::CloseFrame;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::frame::Frame;
use tokio_tungstenite::WebSocketStream;
use crate::app::AppState;
use crate::remote::event::Events;

pub struct WsBehavior {
    state: AppState,
    event_sender: UnboundedSender<(Events, Value)>, // TODO 实现event
    sender: Mutex<UnboundedSender<Message>>,
    addr: SocketAddr
}

#[derive(Serialize, Debug, Clone, Copy)]
struct HeartBeatTemplate {
    time: u64,
}

impl WsBehavior {
    pub fn new(state: AppState, event_sender: UnboundedSender<(Events, Value)>, sender: UnboundedSender<Message>,addr:SocketAddr) -> WsBehavior {
        // let mut es = event_sender.clone();
        // tokio::spawn(async move {
        //     loop {
        //         tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        //         es.send((Events::HeartBeat, serde_json::to_value(HeartBeatTemplate {
        //             time: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
        //         }).unwrap())).await;
        //     }
        // });

        WsBehavior {
            state,
            event_sender,
            sender: Mutex::new(sender),
            addr
        }
    }
}
impl WsBehavior {
    async fn handle_text(&self, msg: String) -> anyhow::Result<()> {
        // TODO 实现action

        info!("received text: {}", msg);
        self.send(Message::Text(msg)).await?;
        Ok(())
    }

    async fn handle_binary(&self, msg: Vec<u8>) -> anyhow::Result<()> {
        todo!()
    }

    async fn handle_ping(&self, msg: Vec<u8>) -> anyhow::Result<()> {
        todo!()
    }

    async fn handle_pong(&self, msg: Vec<u8>) -> anyhow::Result<()> {
        todo!()
    }

    async fn handle_close(&self, msg: Option<CloseFrame<'_>>) -> anyhow::Result<()> {
        info!("websocket close from client: {}",self.addr);
        Ok(())
    }

    async fn handle_frame(&self, frame: Frame) -> anyhow::Result<()> {
        todo!()
    }

    async fn send(&self, msg: Message) -> anyhow::Result<()> {
        let mut guard = self.sender.lock().await;
        guard.send(msg).await?;
        Ok(())
    }

    async fn stop(&self) -> anyhow::Result<()> {
        let close_frame = CloseFrame {
            code: CloseCode::Normal,
            reason: "".into(),
        };
        self.send(Message::Close(Some(close_frame))).await?;
        Ok(())
    }
}

impl WsBehavior {
    pub async fn start(ws: WebSocketStream<TokioIo<Upgraded>>, state: AppState, mut cancel_token: Receiver<bool>,peer_addr:SocketAddr) -> anyhow::Result<()> {
        let (mut outgoing, mut incoming) = ws.split();

        let (outgoing_tx, mut outgoing_rx) = unbounded();

        let (event_tx, mut event_rx) = unbounded();

        let ws_behavior = WsBehavior::new(state, event_tx, outgoing_tx,peer_addr);

        let incoming_loop = async move {
            loop {
                select! {
                    msg = incoming.next() => {
                        if let Some(Ok(m)) = msg{
                            match m {
                                Message::Text(text) => ws_behavior.handle_text(text).await,
                                Message::Binary(bin) => ws_behavior.handle_binary(bin).await,
                                Message::Ping(ping) => ws_behavior.handle_ping(ping).await,
                                Message::Pong(pong) => ws_behavior.handle_pong(pong).await,
                                Message::Close(close) => ws_behavior.handle_close(close).await,
                                Message::Frame(frame) => ws_behavior.handle_frame(frame).await
                            }?
                        }
                        else {break;}
                    }

                    _ = cancel_token.changed() => {
                        ws_behavior.stop().await?;
                        break;
                    }
                }
            }
            anyhow::Ok(())
        };

        let outgoing_loop = async move {
            loop {
                select! {
                    m = outgoing_rx.next() => {
                        if let Some(m) = m{
                            outgoing.send(m).await?;
                        }
                        else {break;}
                    }
                    e = event_rx.next() => {
                        if let Some((event, data)) = e{
                            let text = json!({
                                "event": event.to_string(),
                                "data": data
                            }).to_string();

                            outgoing.send(Message::text(text)).await?;
                        }
                        else {break;}
                    }
                }
            }
            Ok(())
        };

        let incoming_task = tokio::spawn(incoming_loop).map_err(|e: JoinError| anyhow!("incoming task error: {}", e));

        tokio::try_join!(
            incoming_task,
            outgoing_loop
        )?;

        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        Ok(())
    }
}


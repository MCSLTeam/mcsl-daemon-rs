use std::net::SocketAddr;

use anyhow::anyhow;
use futures::{SinkExt, StreamExt, TryFutureExt};
use hyper::upgrade::Upgraded;
use hyper_util::rt::TokioIo;
use log::{debug, info};
use serde_json::{json, Value};
use tokio::select;
use tokio::sync::mpsc::{error::SendError, unbounded_channel, UnboundedSender};
use tokio::task::JoinError;
use tokio_tungstenite::tungstenite::{
    protocol::{frame::coding::CloseCode, frame::Frame, CloseFrame},
    Message,
};
use tokio_tungstenite::WebSocketStream;

use super::event::Events;
use crate::app::AppResources;
use crate::remote::protocols::Protocol;

pub struct WsBehavior {
    #[allow(dead_code)]
    app_resources: AppResources,

    #[allow(dead_code)]
    event_sender: UnboundedSender<(Events, Value)>, // TODO 实现event

    sender: UnboundedSender<Message>,
    addr: SocketAddr,
}

impl WsBehavior {
    fn new(
        app_resources: AppResources,
        event_sender: UnboundedSender<(Events, Value)>,
        sender: UnboundedSender<Message>,
        addr: SocketAddr,
    ) -> WsBehavior {
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
            app_resources,
            event_sender,
            sender,
            addr,
        }
    }
}
impl WsBehavior {
    fn handle_text(&self, msg: String) -> anyhow::Result<()> {
        // TODO 实现action

        info!("received text: {}", msg);

        let actions = self.app_resources.protocol_v1.clone();
        let sender = self.sender.downgrade();
        tokio::spawn(async move {
            let text = actions.process_text(msg.as_ref()).await;
            if text.is_none() {
                return;
            }
            let text = text.unwrap();
            if let Some(sender) = sender.upgrade() {
                if let Err(msg) = sender.send(Message::Text(text)) {
                    debug!("could not send message due to ws sender dropped: {}", msg);
                }
            } else {
                debug!("could not send message due to ws sender dropped: {}", text);
            }
        });
        Ok(())
    }

    fn handle_binary(&self, msg: Vec<u8>) -> anyhow::Result<()> {
        todo!()
    }

    fn handle_ping(&self, msg: Vec<u8>) -> anyhow::Result<()> {
        // auto pong
        self.send(Message::Pong(msg))?;
        Ok(())
    }

    fn handle_closing(&self, msg: Option<CloseFrame<'_>>) -> anyhow::Result<()> {
        info!(
            "[WsBehavior] websocket close from client({}), with reason: {}",
            self.addr,
            msg.map(|f| f.reason).unwrap_or_default()
        );
        Ok(())
    }

    fn handle_frame(&self, frame: Frame) -> anyhow::Result<()> {
        todo!()
    }

    fn send(&self, msg: Message) -> Result<(), SendError<Message>> {
        // let mut guard = self.sender.lock().await;
        // guard.send(msg).await?;
        self.sender.clone().send(msg)
    }

    fn stop(&self) -> anyhow::Result<()> {
        let close_frame = CloseFrame {
            code: CloseCode::Normal,
            reason: "".into(),
        };
        self.send(Message::Close(Some(close_frame)))?;
        Ok(())
    }
}

impl WsBehavior {}

impl WsBehavior {
    pub async fn start(
        ws: WebSocketStream<TokioIo<Upgraded>>,
        app_resources: AppResources,
        peer_addr: SocketAddr,
    ) -> anyhow::Result<()> {
        let (mut outgoing, mut incoming) = ws.split();

        let (outgoing_tx, mut outgoing_rx) = unbounded_channel();

        let (event_tx, mut event_rx) = unbounded_channel();

        let ws_behavior = WsBehavior::new(app_resources.clone(), event_tx, outgoing_tx, peer_addr);

        let cancel_token = app_resources.cancel_token.clone();

        let incoming_loop_func = async move {
            loop {
                select! {
                    msg = incoming.next() => {
                        if let Some(Ok(m)) = msg{
                            match m {
                                Message::Text(text) => ws_behavior.handle_text(text),
                                Message::Binary(bin) => ws_behavior.handle_binary(bin),
                                Message::Ping(ping) => ws_behavior.handle_ping(ping),
                                Message::Close(close) => ws_behavior.handle_closing(close),
                                Message::Frame(frame) => ws_behavior.handle_frame(frame),
                                _ => Ok(())
                            }?
                        }
                        else {break;}
                    }

                    _ = cancel_token.notified() => {
                        ws_behavior.stop()?;
                        info!("websocket connection from {} closed", peer_addr);
                        break;
                    }
                }
            }
            anyhow::Ok(())
        };

        let outgoing_loop = async move {
            loop {
                select! {
                    Some(m) = outgoing_rx.recv() => {
                        match m {
                            Message::Close(_)=>{
                                outgoing.send(m).await?;
                                outgoing.close().await?;
                            },
                            _ => outgoing.send(m).await?
                        }
                    }
                    Some((event, data)) = event_rx.recv() => {
                        let text = json!({
                            "event": event.to_string(),
                            "data": data
                        }).to_string();

                        outgoing.send(Message::text(text)).await?;
                    }
                    else => break,
                }
            }
            Ok(())
        };

        let incoming_loop = tokio::spawn(incoming_loop_func)
            .map_err(|e: JoinError| anyhow!("incoming task error: {}", e));

        tokio::try_join!(incoming_loop, outgoing_loop).map(|_| ())
    }
}

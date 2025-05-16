use chrono::{DateTime, Utc};
use log::{debug, info};
use std::ops::Deref;
use std::sync::{Arc, LazyLock};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::config::AppConfig;
use crate::drivers::websocket::WsConnManager;
use crate::drivers::GracefulShutdown;
use crate::protocols::v1::ProtocolV1;
use crate::protocols::Protocols;
use crate::storage::Files;
use tokio::sync::Notify;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
static START_TIME: LazyLock<DateTime<Utc>> = LazyLock::new(Utc::now);

pub struct ApplicationState {
    pub stop_notify: Arc<Notify>,
    pub protocols: Protocols,
    pub protocol_v1: Arc<ProtocolV1>,
    pub ws_connections: Mutex<Vec<JoinHandle<()>>>,
    pub ws_conn_manager: WsConnManager,
}
pub type AppState = Arc<ApplicationState>;

pub fn get_start_time() -> &'static DateTime<Utc> {
    START_TIME.deref()
}

fn init_app_state() -> AppState {
    let config = AppConfig::get();
    debug!(
        "config loaded: {}",
        serde_json::to_string_pretty(&config).unwrap()
    );

    let files = Files::new(config.protocols.clone());
    let protocol_v1 = Arc::new(ProtocolV1::new(files)); // v1 protocol resources
    let protocols = Protocols::combine(config.protocols.enabled.as_ref());

    let resources = ApplicationState {
        protocol_v1,
        protocols,
        ws_connections: Mutex::new(vec![]),
        stop_notify: Arc::new(Notify::new()),
        ws_conn_manager: WsConnManager::new(),
    };
    Arc::new(resources)
}

pub async fn run_app() -> anyhow::Result<()> {
    let _ = get_start_time();

    let state = init_app_state();
    let mut gs = GracefulShutdown::new();

    AppConfig::get()
        .drivers
        .enabled
        .iter()
        .for_each(|driver_type| gs.add_driver(driver_type.new_driver(state.clone())));

    gs.watch(state.stop_notify.clone()).await;
    info!("Bye.");
    Ok(())
}

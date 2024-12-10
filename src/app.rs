use std::sync::Arc;

use log::{debug, info};
use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::drivers::GracefulShutdown;
use crate::protocols::v1::ProtocolV1;
use crate::protocols::Protocols;
use crate::storage::{AppConfig, Files};
use crate::user::{Users, UsersManager};
use tokio::sync::Notify;

pub struct Resources {
    pub app_config: AppConfig,
    pub users: Users,
    pub cancel_token: Arc<Notify>,
    pub protocols: Protocols,
    pub protocol_v1: Arc<ProtocolV1>,
    pub ws_handlers: Mutex<Vec<JoinHandle<()>>>,
}

pub type AppResources = Arc<Resources>;

async fn init_app_res() -> anyhow::Result<AppResources> {
    let config = AppConfig::load();
    debug!(
        "config loaded: {}",
        serde_json::to_string_pretty(&config).unwrap()
    );

    let files = Files::new(config.protocols.clone());
    let protocol_v1 = Arc::new(ProtocolV1::new(files)); // v1 protocol resources
    let protocols = Protocols::combine(config.protocols.enabled.as_ref());

    let users = Users::build("users.db").await?;
    users.fix_admin().await?;
    debug!(
        "users loaded: {:?}",
        Vec::from_iter(users.get_users().await?.keys())
    );

    let resources = Resources {
        app_config: config,
        users,
        protocol_v1,
        protocols,
        ws_handlers: Mutex::new(vec![]),
        cancel_token: Arc::new(Notify::new()),
    };
    Ok(Arc::new(resources))
}

pub async fn run_app() -> anyhow::Result<()> {
    let resources = init_app_res().await?;
    let mut gs = GracefulShutdown::new();

    resources
        .app_config
        .drivers
        .enabled
        .iter()
        .for_each(|driver_type| gs.add_driver(driver_type.new_driver(resources.clone())));

    gs.watch().await;
    info!("Bye.");
    Ok(())
}

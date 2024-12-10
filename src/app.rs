use std::sync::Arc;

use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::remote::drivers::{GracefulShutdown, WsDriverBuilder};
use crate::remote::protocols::v1::ProtocolV1;
use crate::remote::protocols::Protocols;
use crate::storage::{AppConfig, Files};
use crate::user::Users;
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
    let config = AppConfig::new();
    let users = Users::build("users.db").await?;
    let files = Files::new(config.protocols.clone());
    let protocol_v1 = Arc::new(ProtocolV1::new(files));

    let protocols = Protocols::combine(config.protocols.enabled.as_ref());

    users.fix_admin().await?;

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

    gs.add_driver(WsDriverBuilder::new().with_resources(resources).build()?);

    gs.watch().await;
    Ok(())
}

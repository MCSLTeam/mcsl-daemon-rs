use std::sync::Arc;

use tokio::sync::Mutex;
use tokio::task::JoinHandle;

use crate::remote::drivers::{GracefulShutdown, WsDriverBuilder};
use crate::remote::protocols::v1::ProtocolV1;
use crate::remote::protocols::V1;
use crate::storage::{AppConfig, Files};
use crate::user::Users;
use tokio::sync::Notify;

pub struct Resources {
    pub app_config: AppConfig,
    pub users: Users,
    pub files: Arc<Files>,
    pub cancel_token: Arc<Notify>,
    pub protocol_v1: Arc<ProtocolV1>,
    pub ws_handlers: Mutex<Vec<JoinHandle<()>>>,
}

pub type AppResources = Arc<Resources>;

async fn init_app_res() -> anyhow::Result<AppResources> {
    let config = AppConfig::new();
    let users = Users::build("users.db").await?;
    let files = Arc::new(Files::new(config.clone()));
    let protocol_v1 = Arc::new(ProtocolV1::new(files.clone()));

    users.fix_admin().await?;

    let resources = Resources {
        app_config: config,
        users,
        files,
        protocol_v1,
        ws_handlers: Mutex::new(vec![]),
        cancel_token: Arc::new(Notify::new()),
    };
    Ok(Arc::new(resources))
}

pub async fn run_app() -> anyhow::Result<()> {
    let resources = init_app_res().await?;
    let mut gs = GracefulShutdown::new();

    gs.add_driver(
        WsDriverBuilder::new()
            .with_resources(resources)
            .with_protocol_set(V1)
            .build()?,
    );

    gs.watch().await;
    Ok(())
}

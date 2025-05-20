use crate::management::instance::Instance;
use crate::management::strategy::InstanceProcessStrategy;
use crate::management::strategy::InstanceStrategy;
use anyhow::bail;
use lazy_static::lazy_static;
use mcsl_protocol::management::instance::{InstanceReport, InstanceStatus};
use regex::Regex;
use tokio::sync::broadcast;

lazy_static! {
    static ref DONE_PATTERN: Regex =
        Regex::new(r#"Done \(\d+\.\d{1,3}s\)! For help, type ["']help["']$"#)
            .expect("Failed to compile DONE_PATTERN regex");
}

pub trait StrategyConstructor {
    fn new() -> impl InstanceStrategy + InstanceProcessStrategy + Send + Sync;
}
pub struct UniversalInstanceStrategy {}
pub struct MinecraftInstanceStrategy {}

#[async_trait::async_trait]
impl InstanceStrategy for UniversalInstanceStrategy {
    async fn get_report(&self, this: &Instance) -> InstanceReport {
        let status = {
            let state = this.state.read().await;
            state.status.clone()
        };

        let config = this.get_config();

        InstanceReport {
            status: status.clone(),
            config,
            properties: std::collections::HashMap::default(),
            player: vec![],
            performance_counter: this.get_process_metrics().await,
        }
    }

    async fn stop(&self, this: &Instance) -> anyhow::Result<()> {
        match this.state.write().await.process {
            None => {
                bail!("Instance is not running")
            }
            Some(ref mut process) => process.term(),
        }
    }
}

impl InstanceProcessStrategy for UniversalInstanceStrategy {
    fn on_process_start(&self, statue_tx: &broadcast::Sender<InstanceStatus>) {
        let _ = statue_tx.send(InstanceStatus::Running);
    }

    fn on_line_received(&self, line: &str, status_tx: &broadcast::Sender<InstanceStatus>) {}
}

impl StrategyConstructor for UniversalInstanceStrategy {
    fn new() -> impl InstanceStrategy + InstanceProcessStrategy + Send + Sync {
        Self {}
    }
}

#[async_trait::async_trait]
impl InstanceStrategy for MinecraftInstanceStrategy {
    async fn get_report(&self, this: &Instance) -> InstanceReport {
        let status = {
            let state = this.state.read().await;
            state.status.clone()
        };

        let config = this.get_config();

        InstanceReport {
            status: status.clone(),
            config,
            properties: std::collections::HashMap::default(),
            player: vec![],
            performance_counter: this.get_process_metrics().await,
        }
    }

    async fn stop(&self, this: &Instance) -> anyhow::Result<()> {
        let _ = this.send("stop\n".into());
        Ok(())
    }
}

impl InstanceProcessStrategy for MinecraftInstanceStrategy {
    fn on_process_start(&self, statue_tx: &broadcast::Sender<InstanceStatus>) {
        let _ = statue_tx.send(InstanceStatus::Starting);
    }

    fn on_line_received(&self, line: &str, status_tx: &broadcast::Sender<InstanceStatus>) {
        let line = line.trim_end();
        if DONE_PATTERN.is_match(line) {
            let _ = status_tx.send(InstanceStatus::Running);
        } else if line.contains("Stopping the server") {
            let _ = status_tx.send(InstanceStatus::Stopping);
        } else if line.contains("Minecraft has crashed") {
            let _ = status_tx.send(InstanceStatus::Crashed);
        }
    }
}

impl StrategyConstructor for MinecraftInstanceStrategy {
    fn new() -> impl InstanceStrategy + InstanceProcessStrategy + Send + Sync {
        Self {}
    }
}

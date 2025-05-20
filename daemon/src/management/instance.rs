use crate::management::comm::InstanceProcess;
use crate::management::config::InstanceConfigExt;
use crate::management::strategy::{InstanceProcessStrategy, InstanceStrategy, StrategyConstructor};
use anyhow::{bail, Result};
use log::info;
use mcsl_protocol::management::instance::{
    InstanceConfig, InstanceProcessMetrics, InstanceReport, InstanceStatus,
};
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::select;
use tokio::sync::broadcast::error::{RecvError, SendError};
use tokio::sync::{broadcast, RwLock};

pub const INST_CFG_FILE_NAME: &str = "daemon_instance.json";

// Ensure Instance is Sync and Send
unsafe impl Sync for Instance {}
unsafe impl Send for Instance {}

// 实例状态
pub(super) struct InstanceState {
    pub(super) config: InstanceConfig,
    last_config_modified: Option<SystemTime>,
    pub(super) status: InstanceStatus,
    pub(super) process: Option<InstanceProcess>,
}

impl InstanceState {
    pub(super) fn new(config: InstanceConfig) -> Self {
        Self {
            config,
            last_config_modified: None,
            status: InstanceStatus::Stopped,
            process: None,
        }
    }

    pub(super) fn has_config_changed(&self, config_path: &Path) -> bool {
        let current_metadata = std::fs::metadata(config_path);
        match (current_metadata, self.last_config_modified) {
            (Ok(meta), Some(last)) => meta.modified().ok() != Some(last),
            (Ok(_), None) => true,
            (Err(_), Some(_)) => true,
            (Err(_), None) => false,
        }
    }

    pub(super) fn reload_config(&mut self, config_path: &Path) -> Result<()> {
        let data = std::fs::read_to_string(config_path)
            .map_err(|e| anyhow::anyhow!("Failed to read config: {}", e))?;
        let new_config = serde_json::from_str::<InstanceConfig>(&data)
            .map_err(|e| anyhow::anyhow!("Failed to parse config: {}", e))?;
        if new_config.uuid != self.config.uuid {
            return Err(anyhow::anyhow!("UUID changed, ignoring update"));
        }
        self.config = new_config;
        self.last_config_modified = std::fs::metadata(config_path)
            .ok()
            .and_then(|m| m.modified().ok());
        Ok(())
    }
}

// Instance struct as provided
pub struct Instance {
    pub(super) state: Arc<RwLock<InstanceState>>,
    pub(super) log_tx: broadcast::Sender<String>,
    pub(super) input_tx: broadcast::Sender<String>,
    pub(super) status_tx: broadcast::Sender<InstanceStatus>,
    strategy: Arc<dyn InstanceStrategy + Send + Sync>,
    process_strategy: Arc<dyn InstanceProcessStrategy + Send + Sync>,
}
impl Instance {
    pub fn new<S>(config: InstanceConfig) -> Self
    where
        S: StrategyConstructor + InstanceStrategy + InstanceProcessStrategy + 'static + Send + Sync,
    {
        let (log_tx, _) = broadcast::channel(256);
        let (input_tx, _) = broadcast::channel(32);
        let (status_tx, _) = broadcast::channel(32);
        let state = Arc::new(RwLock::new(InstanceState::new(config)));

        let strategy = Arc::new(S::new());
        Self {
            state,
            log_tx,
            input_tx,
            status_tx,
            strategy: strategy.clone() as Arc<dyn InstanceStrategy + Send + Sync>,
            process_strategy: strategy as Arc<dyn InstanceProcessStrategy + Send + Sync>,
        }
    }
}

// Trait to enable dynamic dispatch for instances
impl Instance {
    pub fn get_config(&self) -> InstanceConfig {
        let mut state = self.state.blocking_write();
        let config_path = Path::new(&state.config.get_working_dir()).join(INST_CFG_FILE_NAME);
        if matches!(
            state.status,
            InstanceStatus::Stopped | InstanceStatus::Crashed
        ) && state.has_config_changed(&config_path)
        {
            if let Err(e) = state.reload_config(&config_path) {
                eprintln!("Failed to reload config: {}", e);
            }
        }
        state.config.clone()
    }

    pub fn get_status(&self) -> InstanceStatus {
        self.state.blocking_read().status.clone()
    }
    pub fn get_log_rx(&self) -> broadcast::Receiver<String> {
        self.log_tx.subscribe()
    }
    pub fn get_status_rx(&self) -> broadcast::Receiver<InstanceStatus> {
        self.status_tx.subscribe()
    }
    pub async fn get_process_metrics(&self) -> InstanceProcessMetrics {
        let state = self.state.read().await;
        match state.process.as_ref() {
            Some(proc) => {
                let monitor = proc.monitor.clone();
                drop(state);
                monitor.get_process_metrics().await
            }
            None => InstanceProcessMetrics::default(),
        }
    }

    pub async fn get_report(&self) -> InstanceReport {
        self.strategy.get_report(self).await
    }

    // TODO apply process_strategy
    pub async fn start(&self) -> Result<()> {
        let mut state = self.state.write().await;
        if state.process.is_some() {
            return Err(anyhow::anyhow!("Process already running"));
        }

        let config_path = Path::new(&state.config.get_working_dir()).join(INST_CFG_FILE_NAME);
        if matches!(
            state.status,
            InstanceStatus::Stopped | InstanceStatus::Crashed
        ) && state.has_config_changed(&config_path)
        {
            state.reload_config(&config_path)?;
        }

        tokio::spawn({
            let state = self.state.clone();
            let mut status_rx = self.status_tx.subscribe();
            async move {
                loop {
                    match status_rx.recv().await {
                        Ok(status) => {
                            info!("InstanceStatus changed to {:?}", status);
                            state.write().await.status = status.clone();
                        }
                        Err(err) => match err {
                            RecvError::Closed => break,
                            RecvError::Lagged(_) => continue,
                        },
                    }
                }
            }
        });

        let process = InstanceProcess::start(
            &state.config,
            state.config.is_mc_server(),
            self.log_tx.clone(),
            self.input_tx.subscribe(),
            self.status_tx.clone(),
            self.process_strategy.clone(),
        )
        .await
        .map_err(|e| anyhow::anyhow!("Failed to start process: {}", e))?;

        drop(state);

        let mut collector = self.log_tx.subscribe();
        let mut log_collected = vec![];

        loop {
            select! {
                _ = tokio::time::sleep(Duration::from_millis(500)) => {
                    break;
                }

                line = collector.recv() => {
                    if let Ok(line) = line {
                        log_collected.push(line);
                    }
                }
            }
        }

        if !process.exited() {
            let mut state = self.state.write().await;
            state.process = Some(process);
            Ok(())
        } else {
            bail!("Process exited: \n{}", log_collected.join("\n"));
        }
    }

    pub async fn stop(&self) -> Result<()> {
        self.strategy.stop(self).await
    }

    pub fn kill(&self) {
        let process = {
            let mut state = self.state.blocking_write();
            state.process.take()
        };
        if let Some(process) = process {
            process.kill()
        }
    }

    pub fn send(&self, msg: String) -> std::result::Result<usize, SendError<String>> {
        self.input_tx.send(msg.to_string())
    }
}

// #[async_trait::async_trait]
// impl<TInst: TInstance + 'static> UniversalInstance for Instance<TInst> {
//     fn as_any(&self) -> &dyn Any {
//         self
//     }
//
//     fn get_config(&self) -> InstanceConfig {
//         let mut state = self.state.blocking_write();
//         let config_path = Path::new(&state.config.get_working_dir()).join(INST_CFG_FILE_NAME);
//         if matches!(
//             state.status,
//             InstanceStatus::Stopped | InstanceStatus::Crashed
//         ) && state.has_config_changed(&config_path)
//         {
//             if let Err(e) = state.reload_config(&config_path) {
//                 eprintln!("Failed to reload config: {}", e);
//             }
//         }
//
//         state.config.clone()
//     }
//
//     fn get_status(&self) -> InstanceStatus {
//         self.state.blocking_read().status.clone()
//     }
//
//     fn get_log_rx(&self) -> broadcast::Receiver<String> {
//         self.log_tx.subscribe()
//     }
//
//     fn get_status_rx(&self) -> broadcast::Receiver<InstanceStatus> {
//         self.status_tx.subscribe()
//     }
//
//     async fn get_monitor_data(&self) -> InstancePerformanceCounter {
//         let state = self.state.read().await;
//         match state.process.as_ref() {
//             Some(proc) => {
//                 let monitor = proc.monitor.clone();
//                 drop(state);
//                 monitor.get_monitor_data().await
//             }
//             None => InstancePerformanceCounter::default(),
//         }
//     }
//
//     async fn get_report(&self) -> InstanceReport {
//         let state = self.state.read().await;
//         let status = state.status.clone();
//
//         drop(state);
//
//         InstanceReport {
//             status: status.clone(),
//             config: self.get_config(),
//             properties: std::collections::HashMap::default(),
//             player: vec![],
//             performance_counter: self.get_monitor_data().await,
//         }
//     }
//
//     async fn start(&self) -> Result<()> {
//         let mut state = self.state.write().await;
//         if state.process.is_some() {
//             return Err(anyhow::anyhow!("Process already running"));
//         }
//
//         let config_path = Path::new(&state.config.get_working_dir()).join(INST_CFG_FILE_NAME);
//         if matches!(
//             state.status,
//             InstanceStatus::Stopped | InstanceStatus::Crashed
//         ) && state.has_config_changed(&config_path)
//         {
//             state.reload_config(&config_path)?;
//         }
//
//         tokio::spawn({
//             let state = self.state.clone();
//             let mut status_rx = self.status_tx.subscribe();
//             async move {
//                 loop {
//                     match status_rx.recv().await {
//                         Ok(status) => {
//                             info!("InstanceStatus changed to {:?}", status);
//                             state.write().await.status = status.clone();
//                         }
//                         Err(err) => match err {
//                             RecvError::Closed => break,
//                             RecvError::Lagged(_) => continue,
//                         },
//                     }
//                 }
//             }
//         });
//
//         let process = InstanceProcess::start(
//             &state.config,
//             state.config.is_mc_server(),
//             self.log_tx.clone(),
//             self.input_tx.subscribe(),
//             self.status_tx.clone(),
//         )
//         .await
//         .map_err(|e| anyhow::anyhow!("Failed to start process: {}", e))?;
//
//         drop(state);
//
//         let mut collector = self.log_tx.subscribe();
//         let mut log_collected = vec![];
//
//         loop {
//             select! {
//                 _ = tokio::time::sleep(Duration::from_millis(500)) => {
//                     break;
//                 }
//
//                 line = collector.recv() => {
//                     if let Ok(line) = line {
//                         log_collected.push(line);
//                     }
//                 }
//             }
//         }
//
//         if !process.exited() {
//             let mut state = self.state.write().await;
//             state.process = Some(process);
//             Ok(())
//         } else {
//             anyhow::bail!("Process exited: \n{}", log_collected.join("\n"));
//         }
//     }
//
//     fn stop(&self) -> Result<()> {
//         if let Some(ref mut process) = self.state.blocking_write().process.take() {
//             process.term()
//         } else {
//             Ok(())
//         }
//     }
//     fn kill(&self) {
//         if let Some(process) = self.state.blocking_write().process.take() {
//             process.kill();
//         }
//     }
//
//     fn send(&self, msg: &str) -> core::result::Result<usize, SendError<String>> {
//         self.input_tx.send(msg.to_string())
//     }
// }

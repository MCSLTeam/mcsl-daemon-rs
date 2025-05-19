use crate::management::comm::InstanceProcess;
use crate::management::config::InstanceConfigExt;
use anyhow::Result;
use mcsl_protocol::management::instance::{
    InstanceConfig, InstancePerformanceCounter, InstanceReport, InstanceStatus,
};
use std::any::Any;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::select;
use tokio::sync::broadcast::error::SendError;
use tokio::sync::{broadcast, mpsc, RwLock};
use uuid::Uuid;

pub const INST_CFG_FILE_NAME: &'static str = "daemon_instance.json";

// Ensure Instance is Sync and Send
unsafe impl<TInst: TInstance> Sync for Instance<TInst> {}
unsafe impl<TInst: TInstance> Send for Instance<TInst> {}

// 实例状态
struct InstanceState {
    config: InstanceConfig,
    last_config_modified: Option<SystemTime>,
    status: InstanceStatus,
    process: Option<InstanceProcess>,
}

impl InstanceState {
    fn new(config: InstanceConfig) -> Self {
        Self {
            config,
            last_config_modified: None,
            status: InstanceStatus::Stopped,
            process: None,
        }
    }

    fn has_config_changed(&self, config_path: &Path) -> bool {
        let current_metadata = std::fs::metadata(config_path);
        match (current_metadata, self.last_config_modified) {
            (Ok(meta), Some(last)) => meta.modified().ok() != Some(last),
            (Ok(_), None) => true,
            (Err(_), Some(_)) => true,
            (Err(_), None) => false,
        }
    }

    fn reload_config(&mut self, config_path: &Path) -> Result<()> {
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

// Trait for instances
pub trait TInstance {}

pub struct Universal;
pub struct Minecraft;

impl TInstance for Universal {}
impl TInstance for Minecraft {}

// Instance struct as provided
pub struct Instance<TInst>
where
    TInst: TInstance,
{
    state: Arc<RwLock<InstanceState>>,
    log_tx: broadcast::Sender<String>,
    input_tx: broadcast::Sender<String>,
    _marker: std::marker::PhantomData<TInst>,
}

impl<TInst: TInstance> Instance<TInst> {
    pub fn new(config: InstanceConfig) -> Self {
        let (log_tx, _) = broadcast::channel(1024);
        let (input_tx, _) = broadcast::channel(1024);
        let state = Arc::new(RwLock::new(InstanceState::new(config)));
        Self {
            state,
            log_tx,
            input_tx,
            _marker: std::marker::PhantomData,
        }
    }

    // Example async method
    pub async fn do_work(&self) {
        // Simulate time-consuming operation
        tokio::time::sleep(Duration::from_millis(1000)).await;
        println!("Work completed for {:?}", std::any::type_name::<TInst>());
    }
}

// Trait to enable dynamic dispatch for instances
#[async_trait::async_trait]
pub trait UniversalInstance: Any + Send + Sync {
    fn as_any(&self) -> &dyn Any;

    fn get_config(&self) -> InstanceConfig;

    fn get_status(&self) -> InstanceStatus;
    fn get_log_rx(&self) -> broadcast::Receiver<String>;
    async fn get_monitor_data(&self) -> InstancePerformanceCounter;

    async fn get_report(&self) -> InstanceReport;

    async fn start(&self) -> Result<()>;

    fn stop(&self) -> Result<()>;

    fn kill(&self);

    fn send(&self, msg: &str) -> core::result::Result<usize, SendError<String>>;
}

#[async_trait::async_trait]
impl<TInst: TInstance + 'static> UniversalInstance for Instance<TInst> {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn get_config(&self) -> InstanceConfig {
        let mut state = self.state.blocking_write();
        let config_path = Path::new(&state.config.get_working_dir()).join(INST_CFG_FILE_NAME);
        if matches!(
            state.status,
            InstanceStatus::Stopped | InstanceStatus::Crashed
        ) {
            if state.has_config_changed(&config_path) {
                if let Err(e) = state.reload_config(&config_path) {
                    eprintln!("Failed to reload config: {}", e);
                }
            }
        }
        state.config.clone()
    }

    fn get_status(&self) -> InstanceStatus {
        self.state.blocking_read().status.clone()
    }

    fn get_log_rx(&self) -> broadcast::Receiver<String> {
        self.log_tx.subscribe()
    }

    async fn get_monitor_data(&self) -> InstancePerformanceCounter {
        let state = self.state.read().await;
        match state.process.as_ref() {
            Some(proc) => {
                let monitor = proc.monitor.clone();
                drop(state);
                monitor.get_monitor_data().await
            }
            None => InstancePerformanceCounter::default(),
        }
    }

    async fn get_report(&self) -> InstanceReport {
        let state = self.state.read().await;
        let status = state.status.clone();

        drop(state);

        InstanceReport {
            status: status.clone(),
            config: self.get_config(),
            properties: std::collections::HashMap::default(),
            player: vec![],
            performance_counter: self.get_monitor_data().await,
        }
    }

    async fn start(&self) -> Result<()> {
        let mut state = self.state.write().await;
        if state.process.is_some() {
            return Err(anyhow::anyhow!("Process already running"));
        }

        let config_path = Path::new(&state.config.get_working_dir()).join(INST_CFG_FILE_NAME);
        if matches!(
            state.status,
            InstanceStatus::Stopped | InstanceStatus::Crashed
        ) {
            if state.has_config_changed(&config_path) {
                state.reload_config(&config_path)?;
            }
        }

        let (status_tx, mut status_rx) = mpsc::channel::<InstanceStatus>(10);
        let state_clone = self.state.clone();
        tokio::spawn({
            async move {
                while let Some(new_status) = status_rx.recv().await {
                    state_clone.write().await.status = new_status;
                }
            }
        });

        let process = InstanceProcess::start(
            &state.config,
            false,
            self.log_tx.clone(),
            self.input_tx.subscribe(),
            status_tx,
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
            anyhow::bail!("Process exited: \n{}", log_collected.join("\n"));
        }
    }

    fn stop(&self) -> Result<()> {
        self.kill();
        Ok(())
    }
    fn kill(&self) {
        if let Some(process) = &self.state.blocking_read().process {
            let _ = process.kill();
        }
    }

    fn send(&self, msg: &str) -> core::result::Result<usize, SendError<String>> {
        self.log_tx.send(msg.to_string())
    }
}

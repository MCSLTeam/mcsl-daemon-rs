use anyhow::{anyhow, bail, Result};
use cached::proc_macro::cached;
use lazy_static::lazy_static;
use log::{debug, warn};
use regex::Regex;
use std::ffi::OsString;
use std::path::Path;
use std::sync::{atomic, Arc};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::select;
use tokio::sync::{broadcast, mpsc, oneshot};

use crate::management::comm::process_helper::ProcessHelper;
use crate::management::config::InstanceConfigExt;
use crate::management::strategy::InstanceProcessStrategy;
use mcsl_protocol::management::instance::{InstanceConfig, InstanceProcessMetrics, InstanceStatus};

lazy_static! {
    static ref DONE_PATTERN: Regex =
        Regex::new(r#"Done \(\d+\.\d{1,3}s\)! For help, type ["']help["']$"#)
            .expect("Failed to compile DONE_PATTERN regex");
}

pub struct ProcessStartInfo {
    pub target: String,
    pub args: Vec<String>,
    pub envs: std::collections::HashMap<OsString, OsString>,
}

// 进程监控器
#[derive(Clone)]
pub struct ProcessMonitor {
    process_id: u32,
}

#[cached(time = 2, size = 128)]
async fn _get_process_metrics(pid: u32) -> InstanceProcessMetrics {
    match ProcessHelper::get_process_metrics(pid).await {
        Ok(pc) => pc,
        Err(err) => {
            warn!("Failed to get process metric: {}", err);
            InstanceProcessMetrics::default()
        }
    }
}
impl ProcessMonitor {
    pub fn new(process_id: u32) -> Self {
        ProcessMonitor { process_id }
    }

    pub async fn get_process_metrics(self) -> InstanceProcessMetrics {
        _get_process_metrics(self.process_id).await
    }
}

// 实例进程
pub struct InstanceProcess {
    process_id: u32,
    exited: Arc<atomic::AtomicBool>,
    term_signal: Option<oneshot::Sender<bool>>,
    log_tx: broadcast::Sender<String>,
    status_tx: broadcast::Sender<InstanceStatus>,
    pub monitor: ProcessMonitor,
}

impl InstanceProcess {
    pub async fn start(
        config: &InstanceConfig,
        is_mc_server: bool,
        log_tx: broadcast::Sender<String>,
        mut input_rx: broadcast::Receiver<String>,
        status_tx: broadcast::Sender<InstanceStatus>,
        strategy: Arc<dyn InstanceProcessStrategy + Send + Sync>,
    ) -> Result<Self, std::io::Error> {
        let start_info = config.get_start_info();
        let working_dir = config.get_working_dir();
        let mut cmd = Command::new(start_info.target);
        cmd.args(start_info.args)
            .current_dir(working_dir)
            .envs(start_info.envs)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);

        let origin_path = std::env::var("PATH").unwrap_or_default();
        let java_dir = Path::new(&config.java_path)
            .parent()
            .unwrap()
            .to_string_lossy();
        cmd.env(
            "PATH",
            if cfg!(windows) {
                format!("{};{}", java_dir, origin_path)
            } else {
                format!("{}:{}", java_dir, origin_path)
            },
        );
        for (key, value) in &config.env {
            cmd.env(key, value);
        }

        // prepare process resource
        let mut process = cmd.spawn()?;

        // update process status
        strategy.on_process_start(&status_tx);

        let process_id = process.id().unwrap_or(0);

        #[cfg(not(windows))]
        let server_process_id = process_id;

        #[cfg(windows)]
        let server_process_id = process
            .id()
            .map(|id| {
                ProcessHelper::child_id(id, Some(&config.target))
                    .map(|ids| ids.first().into())
                    .ok()
            })
            .flatten()
            .unwrap_or(0);

        let (stop_tx, term_rx) = oneshot::channel();
        let exited = Arc::new(atomic::AtomicBool::new(false));
        let monitor = ProcessMonitor::new(server_process_id);

        let (output_tx, output_rx) = mpsc::channel::<String>(100);

        let stdout = process.stdout.take().unwrap();
        let stderr = process.stderr.take().unwrap();
        let mut stdin = process.stdin.take().unwrap();

        tokio::spawn({
            use tokio::io::{AsyncBufReadExt, BufReader};
            let mut stdout = BufReader::new(stdout).lines();
            let mut stderr = BufReader::new(stderr).lines();
            let log_tx = log_tx.clone();
            let status_tx = status_tx.clone();
            let exited = exited.clone();
            let strategy = strategy.clone();

            async move {
                let term_rx_fut = term_rx;
                tokio::pin!(term_rx_fut);
                loop {
                    select! {
                        // 监听进程stdout
                        line = stdout.next_line() => {
                            if let Ok(Some(line)) = line {
                                if is_mc_server {
                                    strategy.on_line_received(&line,&status_tx);
                                }
                                let _ = log_tx.send(line).ok();
                            }
                        }
                        // 监听进程stderr
                        line = stderr.next_line() => {
                            if let Ok(Some(line)) = line {
                                let stderr_line = format!("[STDERR] {}", line);
                                if is_mc_server {
                                    strategy.on_line_received(&line,&status_tx);
                                }
                                let _ = log_tx.send(stderr_line).ok();
                            }
                        }
                        // 进程stdin输入
                        line = input_rx.recv() => {
                            if let Ok(line) = line {
                                if let Err(err) = stdin.write_all(line.as_bytes()).await{
                                    warn!("Error while writing to stdin: {}", err);
                                }
                            }
                        }
                        // 等待进程
                        result = process.wait() => {
                            debug!("Process(pid={}) exited with {:?}",process_id ,result);
                            // TODO 若上次为Crashed则不更新Stopped
                            let _ = status_tx.send(InstanceStatus::Stopped);
                            exited.store(true, atomic::Ordering::Relaxed);
                            break;
                        }
                        // 关闭进程
                        force = term_rx_fut.as_mut() => {
                            let force = match force{
                                Ok(force) => force,
                                Err(_) => {break;}
                            };

                            if force{
                                if let Err(err) = process.kill().await {
                                    warn!("Kill failed (pid={}): {}", process_id, err);
                                }
                            }else if let Err(err) =  ProcessHelper::term(process_id){
                                warn!("Kill failed (pid={}): {}", process_id, err);
                            }

                            status_tx.send(InstanceStatus::Stopped).ok();
                            exited.store(true, atomic::Ordering::Relaxed);
                            break;
                        }
                    }
                }
            }
        });

        Ok(InstanceProcess {
            process_id,
            exited,
            term_signal: Some(stop_tx),
            log_tx,
            status_tx,
            monitor,
        })
    }

    pub fn kill(mut self) {
        self.term_signal.take().map(|stop| stop.send(true));
    }

    pub fn term(&mut self) -> Result<()> {
        match self.term_signal.take() {
            Some(stop) => stop
                .send(false)
                .map_err(|_| anyhow!("Could not send termination signal")),
            None => {
                bail!("Termination signal sent to stop process")
            }
        }
    }

    pub fn exited(&self) -> bool {
        self.exited.load(atomic::Ordering::SeqCst)
    }
}

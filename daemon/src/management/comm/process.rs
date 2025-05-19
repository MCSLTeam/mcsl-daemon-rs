use anyhow::Result;
use lazy_static::lazy_static;
use log::{error, warn};
use regex::Regex;
use std::ffi::OsString;
use std::path::Path;
use std::sync::{atomic, Arc};
use std::time::Duration;
use tokio::process::Command;
use tokio::select;
use tokio::sync::{broadcast, mpsc, Notify};

use crate::management::config::InstanceConfigExt;
use mcsl_protocol::management::instance::{
    InstanceConfig, InstancePerformanceCounter, InstanceStatus,
};

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
    process_id: i32,
    is_mc_server: bool,
    frequency: Duration,
}

impl ProcessMonitor {
    pub fn new(process_id: i32, is_mc_server: bool, frequency: Duration) -> Self {
        ProcessMonitor {
            process_id,
            is_mc_server,
            frequency,
        }
    }

    // TODO
    pub async fn get_monitor_data(self) -> InstancePerformanceCounter {
        InstancePerformanceCounter {
            cpu: 0.0,
            memory: 0,
        }
    }
}

// 实例进程
pub struct InstanceProcess {
    server_process_id: i32,
    exited: Arc<atomic::AtomicBool>,
    kill_notify: Arc<Notify>,
    log_tx: broadcast::Sender<String>,
    input_rx: broadcast::Receiver<String>,
    status_tx: mpsc::Sender<InstanceStatus>,
    pub monitor: ProcessMonitor,
}

impl InstanceProcess {
    pub async fn start(
        config: &InstanceConfig,
        is_mc_server: bool,
        log_tx: broadcast::Sender<String>,
        input_rx: broadcast::Receiver<String>,
        status_tx: mpsc::Sender<InstanceStatus>,
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
        let server_process_id = process.id().unwrap_or(0) as i32;

        let kill_notify = Arc::new(Notify::new());
        let exited = Arc::new(atomic::AtomicBool::new(false));
        let monitor =
            ProcessMonitor::new(server_process_id, is_mc_server, Duration::from_millis(2000));

        let (output_tx, output_rx) = mpsc::channel::<String>(100);

        let stdout = process.stdout.take().unwrap();
        let stderr = process.stderr.take().unwrap();
        tokio::spawn({
            use tokio::io::{AsyncBufReadExt, BufReader};
            let mut stdout = BufReader::new(stdout).lines();
            let mut stderr = BufReader::new(stderr).lines();
            let log_tx = log_tx.clone();
            let status_tx = status_tx.clone();
            let kill_notify = kill_notify.clone();

            async move {
                loop {
                    select! {
                        line = stdout.next_line() => {
                            if let Ok(Some(line)) = line {
                                if is_mc_server {
                                    Self::process_mc_line(&line,&status_tx).await;
                                }
                                let _ = log_tx.send(line).ok();
                            }
                        }
                        line = stderr.next_line() => {
                            if let Ok(Some(line)) = line {
                                let stderr_line = format!("[STDERR] {}", line);
                                if is_mc_server {
                                    Self::process_mc_line(&line,&status_tx).await;
                                }
                                let _ = log_tx.send(stderr_line).ok();
                            }
                        }
                        result = process.wait() => {
                            if result.is_ok() {
                                let _ = status_tx.send(InstanceStatus::Stopped).await;
                            }
                            break;
                        }
                        _ = kill_notify.notified() => {
                            if let Err(err) = process.kill().await {
                                warn!("Could not kill process (pid={}): {}", server_process_id, err);
                            }
                            let _ = status_tx.send(InstanceStatus::Stopped).await;
                            break;
                        }
                    }
                }
                let result = process.wait().await;
                if result.is_ok() {
                    let _ = status_tx.send(InstanceStatus::Stopped).await;
                }
            }
        });

        // tokio::spawn({
        //     let status_tx = status_tx.clone();
        //     let kill_notify = kill_notify.clone();
        //     let exited = exited.clone();
        //     async move {
        //         let id = process.id().unwrap_or(0) as i32;
        //         select! {
        //             _ = process.wait() => {}
        //             _ = kill_notify.notified() => {
        //                 if let Err(err) = process.start_kill(){
        //                     error!("Could not start kill process(pid={}): {}",id ,err);
        //                 };
        //             }
        //         }
        //
        //         exited.store(true, atomic::Ordering::Relaxed);
        //         // TODO 若上次为Crashed则不更新Stopped
        //         let _ = status_tx.send(InstanceStatus::Stopped).await;
        //     }
        // });

        Ok(InstanceProcess {
            server_process_id,
            input_rx,
            exited,
            kill_notify,
            log_tx,
            status_tx,
            monitor,
        })
    }

    pub fn kill(&self) {
        self.kill_notify.notify_one();
    }

    pub fn exited(&self) -> bool {
        self.exited.load(atomic::Ordering::SeqCst)
    }
}

impl InstanceProcess {
    fn stop_process(id: i32) {}

    fn kill_process(id: i32) {}

    async fn process_mc_line(line: &str, status_tx: &mpsc::Sender<InstanceStatus>) {
        let line = line.trim_end();
        if DONE_PATTERN.is_match(line) {
            let _ = status_tx.send(InstanceStatus::Running).await;
        } else if line.contains("Stopping the server") {
            let _ = status_tx.send(InstanceStatus::Stopping).await;
        } else if line.contains("Minecraft has crashed") {
            let _ = status_tx.send(InstanceStatus::Crashed).await;
        }
    }
}

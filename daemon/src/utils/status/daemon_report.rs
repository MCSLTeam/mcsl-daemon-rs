use crate::app;
use crate::utils::status::get_sys_info;
use mcsl_protocol::status::DaemonReport;

pub async fn get_daemon_report() -> anyhow::Result<DaemonReport> {
    Ok(DaemonReport {
        sys_info: get_sys_info().await?,
        start_time_stamp: app::get_start_time().timestamp() as u64,
    })
}

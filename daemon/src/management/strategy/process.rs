use mcsl_protocol::management::instance::InstanceStatus;
use tokio::sync::broadcast;

pub trait InstanceProcessStrategy {
    fn on_process_start(&self, statue_tx: &broadcast::Sender<InstanceStatus>);
    fn on_line_received(&self, line: &str, status_tx: &broadcast::Sender<InstanceStatus>);
}

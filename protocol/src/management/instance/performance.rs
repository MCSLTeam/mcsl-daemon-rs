use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct InstancePerformanceCounter {
    cpu: f64,
    memory: u64,
}

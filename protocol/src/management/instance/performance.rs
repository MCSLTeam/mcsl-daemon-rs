use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct InstancePerformanceCounter {
    pub cpu: f64,
    pub memory: u64,
}

impl Default for InstancePerformanceCounter {
    fn default() -> Self {
        Self {
            cpu: 0.0,
            memory: 0,
        }
    }
}

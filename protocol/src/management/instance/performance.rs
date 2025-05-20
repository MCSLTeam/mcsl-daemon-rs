use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstanceProcessMetrics {
    pub cpu: f64,
    pub memory: u64,
}

impl Default for InstanceProcessMetrics {
    fn default() -> Self {
        Self {
            cpu: 0.0,
            memory: 0,
        }
    }
}

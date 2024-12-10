use std::fmt::Display;

use serde::Serialize;

#[derive(Serialize)]
pub enum Events {
    HeartBeat,
}

// to snake case
impl Display for Events {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Events::HeartBeat => {
                write!(f, "heart_beat")
            }
        }
    }
}

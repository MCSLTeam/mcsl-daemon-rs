use serde::Serialize;

#[derive(PartialEq, Serialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum ActionStatus {
    Ok,
    Error,
}

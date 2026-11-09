use serde::{Deserialize, Serialize};

/// Where an API key comes from, for display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Team,
    Default,
    Environment,
    Server,
    None,
}

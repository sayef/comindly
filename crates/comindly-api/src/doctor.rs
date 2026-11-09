use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Check {
    pub ok: bool,
    /// Optional extras (signing, SCIP) do not count as problems.
    pub optional: bool,
    pub message: String,
}

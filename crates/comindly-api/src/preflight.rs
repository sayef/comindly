/// A changed file in a repo, with changed line ranges (empty = whole file / unknown).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Change {
    pub repo: String,
    pub path: String,
    pub lines: Vec<(u32, u32)>,
}

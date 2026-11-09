/// One ranked search result.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchHit {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub repo: String,
    /// `path:line`.
    pub location: String,
    pub signature: Option<String>,
    pub score: f32,
    pub deps: usize,
    pub summary: Option<String>,
}

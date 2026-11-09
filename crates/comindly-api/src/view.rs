use std::borrow::Cow;

use serde::{Deserialize, Serialize};

use crate::conventions::{Convention, Manual};
use crate::ledger::Entry;
use crate::manifests::InternalDep;
use crate::model::{Edge, Symbol};

/// The parts of an index a server sends a client; the client rebuilds the rest.
#[derive(Serialize, Deserialize)]
pub struct View<'a> {
    pub symbols: Cow<'a, [Symbol]>,
    pub edges: Cow<'a, [Edge]>,
    /// Ledger entries with the status and trust the server derived.
    pub entries: Cow<'a, [Entry]>,
    pub conventions: Cow<'a, [Convention]>,
    pub manual: Cow<'a, [Manual]>,
    pub deps: Cow<'a, [InternalDep]>,
    pub problem: Option<String>,
}

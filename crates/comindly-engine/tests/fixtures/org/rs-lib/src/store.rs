use crate::db::Db;
use crate::Health;

pub struct Store {
    db: Db,
}

impl Store {
    pub fn new() -> Self {
        Store { db: Db::open() }
    }

    pub fn get(&self, key: &str) -> String {
        self.db.query(key)
    }
}

impl Health for Store {
    fn health(&self) -> String {
        self.get("health")
    }
}

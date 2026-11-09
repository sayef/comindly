pub struct Db;

impl Db {
    pub fn open() -> Db {
        Db
    }

    pub fn query(&self, q: &str) -> String {
        q.to_string()
    }
}

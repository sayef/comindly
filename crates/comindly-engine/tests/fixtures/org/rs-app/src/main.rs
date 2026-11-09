use acme_core::db::{self, Db};
use acme_core::Store;

mod util;

fn banner() {}

fn main() {
    let s = Store::new();
    s.get("k");
    let _d: Db = db::Db::open();
    util::helper();
}

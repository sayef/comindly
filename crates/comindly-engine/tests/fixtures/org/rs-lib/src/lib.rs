pub mod db;
pub mod store;

pub use store::Store;

pub trait Health {
    fn health(&self) -> String;
}


#[derive(Default)]
pub struct DatabasesList {
    databases: Vec<()>,
}

impl DatabasesList {
    pub fn new() -> DatabasesList {
        DatabasesList {
            databases: Vec::new(),
        }
    }
}

/// Represents a pool of databases.
pub trait DatabasePool {
    /// Represents a transaction to the database.
    type Transaction;

    ///
    fn transaction(&self) -> Self::Transaction;
}



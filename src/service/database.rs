use std::any::Any;
use std::ops::{Deref, DerefMut};
use std::sync::Mutex;
use std::thread;

use service::ServiceAccess;
use service::StaticServices;
use postgres;
use openssl;

pub struct DatabasePool {
    url: Option<String>,
    databases: Mutex<Vec<postgres::Connection>>,
}

impl DatabasePool {
    pub fn new<S>(url: S) -> DatabasePool where S: Into<String> {
        DatabasePool {
            url: Some(url.into()),
            databases: Mutex::new(Vec::new()),
        }
    }
}

impl Default for DatabasePool {
    fn default() -> DatabasePool {
        DatabasePool {
            url: None,
            databases: Mutex::new(Vec::new()),
        }
    }
}

pub struct Database<'a> {
    pool: &'a DatabasePool,
    connection: Option<postgres::Connection>,
    rollback: bool,
}

impl<'a> ServiceAccess<'a> for Database<'a> {
    fn load(services: &'a StaticServices, _: &'a Box<Any>) -> Database<'a> {
        let url = services.database.url.as_ref().unwrap();

        let mut connection;
        loop {
            let mut connections = services.database.databases.lock().unwrap();
            
            if connections.len() == 0 {
                let ssl = openssl::ssl::SslMethod::Sslv23;
                let ssl = Box::new(openssl::ssl::SslContext::new(ssl).unwrap());
                let ssl = postgres::SslMode::Require(ssl);
                let new_connec = postgres::Connection::connect(&url[..], &ssl).unwrap();
                connections.push(new_connec);
            }

            connection = connections.remove(0);
            if let Ok(_) = connection.execute("BEGIN", &[]) {
                break;
            }
        }

        Database {
            pool: &services.database,
            connection: Some(connection),
            rollback: false,
        }
    }
}

impl<'a> Database<'a> {
    pub fn commit(mut self) {
        self.rollback = false;
    }

    pub fn rollback(mut self) {
        self.rollback = true;
    }
}

impl<'a> Drop for Database<'a> {
    fn drop(&mut self) {
        let connection = match self.connection.take() {
            Some(c) => c,
            None => return
        };

        if self.rollback || thread::panicking() {
            if let Err(_) = connection.execute("ROLLBACK", &[]) {
                return;
            }
        } else {
            if let Err(_) = connection.execute("COMMIT", &[]) {
                return;
            }
        }

        self.pool.databases.lock().unwrap().push(connection);
    }
}

impl<'a> Deref for Database<'a> {
    type Target = postgres::Connection;

    fn deref(&self) -> &postgres::Connection {
        self.connection.as_ref().unwrap()
    }
}

impl<'a> DerefMut for Database<'a> {
    fn deref_mut(&mut self) -> &mut postgres::Connection {
        self.connection.as_mut().unwrap()
    }
}

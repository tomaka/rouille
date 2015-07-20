use std::any::Any;

pub use self::log::LogProvider;
pub use self::templates::TemplatesCache;

pub mod database;
pub mod log;
pub mod templates;

/// Contains the list of services that exist as long as the server exists.
pub struct StaticServices {
    pub databases: database::DatabasesList,
    pub logs: Box<LogProvider + Send + Sync>,
    pub templates: TemplatesCache,
}

impl Default for StaticServices {
    fn default() -> StaticServices {
        StaticServices {
            databases: Default::default(),
            logs: Box::new(log::term::TermLog::new()),
            templates: Default::default(),
        }
    }
}

pub trait ServiceAccess<'a> {
    fn load(&'a StaticServices, route_params: &'a Box<Any>) -> Self;
}

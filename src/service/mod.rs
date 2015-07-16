pub use self::templates::TemplatesCache;

pub mod database;
pub mod templates;

/// Contains the list of services that exist as long as the server exists.
#[derive(Default)]
pub struct StaticServices {
    pub databases: database::DatabasesList,
    pub templates: TemplatesCache,
}

pub trait ServiceAccess<'a> {
    fn load(&'a StaticServices) -> Self;
}

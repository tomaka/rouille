use hyper::server::response::Response as HyperResponse;

use service::StaticServices;

pub use self::json::JsonOutput;
pub use self::plain_text::PlainTextOutput;
pub use self::redirect::RedirectOutput;
pub use self::template::TemplateOutput;

pub mod json;
pub mod plain_text;
pub mod redirect;
pub mod template;

/// Objects that can serve as a response to the request.
pub trait Output {
    /// Sends the response.
    fn send(self, HyperResponse, &StaticServices);
}

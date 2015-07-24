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

pub enum NoOutput {
    IgnoreRoute,
}

/// Objects that can serve as a response to the request.
pub trait Output {
    /// Sends the response.
    fn send(self, HyperResponse, &StaticServices);
}

impl<O> Output for Result<O, NoOutput> where O: Output {
    fn send(self, response: HyperResponse, services: &StaticServices) {
        if let Ok(output) = self {
            output.send(response, services)
        } else {
            unimplemented!();
        }
    }
}

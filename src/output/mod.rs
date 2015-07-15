use hyper::server::response::Response as HyperResponse;

pub use self::json::JsonOutput;
pub use self::plain_text::PlainTextOutput;

pub mod json;
pub mod plain_text;

/// Objects that can serve as a response to the request.
pub trait Output {
    /// Sends the response.
    fn send(self, HyperResponse);
}

use hyper::method::Method as HyperMethod;
use hyper::uri::RequestUri as HyperRequestUri;

pub mod term;

/// Objects that provide server logs.
pub trait LogProvider {
    /// Logs a request made to the server.
    fn log_request(&self, &HyperMethod, &HyperRequestUri, time_nanoseconds: u64);
}

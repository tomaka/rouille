use hyper::server::request::Request as HyperRequest;

pub mod term;

/// Objects that provide server logs.
pub trait LogProvider {
    /// Logs a request made to the server.
    fn log_request(&self, &HyperRequest);
}

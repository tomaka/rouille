use hyper::server::response::Response as HyperResponse;
use service::StaticServices;
use super::Output;

/// A response that redirects the user to another URL.
pub struct RedirectOutput {
    /// The URL to redirect to.
    pub location: String,
    /// The status code to use.
    pub status_code: u16,
}

impl RedirectOutput {
    /// Builds a `RedirectOutput`.
    ///
    /// ```rust
    /// use rouille::output::RedirectOutput;
    /// let output = RedirectOutput::new("/hello", 302);
    /// ```
    pub fn new<S>(location: S, status_code: u16) -> RedirectOutput where S: Into<String> {
        RedirectOutput {
            location: location.into(),
            status_code: status_code,
        }
    }
}

impl Output for RedirectOutput {
    fn send(self, mut response: HyperResponse, _: &StaticServices) {
        use hyper::header::Location;
        use hyper::status::StatusCode;

        // no parse method?
        *response.status_mut() = match self.status_code {
            301 => StatusCode::MovedPermanently,
            302 => StatusCode::Found,
            303 => StatusCode::SeeOther,
            c => StatusCode::Unregistered(c),
        };

        response.headers_mut().set(Location(self.location));

        let _ = response.send(&[]);
    }
}

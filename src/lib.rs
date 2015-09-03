extern crate rustc_serialize;
extern crate time;
extern crate tiny_http;

pub use assets::match_assets;
pub use log::LogEntry;

use std::str::FromStr;

mod assets;
mod input;
mod log;
mod router;

pub enum RouteError {
    /// Couldn't find a way to handle this request.
    NoRouteFound,

    WrongInput,
}

pub struct Server {
    #[inline]
    server: tiny_http::Server,
}

impl Server {
    pub fn start() -> Server {
        Server {
            server: tiny_http::ServerBuilder::new().with_port(8000).build().unwrap(),
        }
    }
}

impl IntoIterator for Server {
    type Item = Request;
    type IntoIter = IncomingRequests;

    #[inline]
    fn into_iter(self) -> IncomingRequests {
        IncomingRequests { server: self.server }
    }
}

pub struct IncomingRequests {
    server: tiny_http::Server,
}

impl Iterator for IncomingRequests {
    type Item = Request;

    #[inline]
    fn next(&mut self) -> Option<Request> {
        Some(Request {
            request: self.server.recv().unwrap(),
        })
    }
}

/// Represents a request made by the client.
pub struct Request {
    request: tiny_http::Request,
}

impl Request {
    /// Returns the URL requested by the client. It is not decoded and thus can contain `%20` or
    /// other characters.
    #[inline]
    pub fn url(&self) -> &str {
        self.request.url()
    }

    /// Consumes the `Request` and sends the response to the client.
    #[inline]
    pub fn respond(self, response: Response) {
        self.request.respond(response.response)
    }

    /// Utility function similar to `respond`, but builds the "default" response corresponding to
    /// the `RouteError`.
    #[inline]
    pub fn respond_to_error(self, err: &RouteError) {
        let response = match err {
            &RouteError::NoRouteFound => tiny_http::Response::empty(404),
            &RouteError::WrongInput => tiny_http::Response::empty(400),
        };

        self.request.respond(response);
    }
}

/// Contains a prototype of a response. The response is only sent when you call `Request::respond`.
pub struct Response {
    response: tiny_http::ResponseBox,
}

impl Response {
    /// Builds a `Response` that redirects the user to another URL.
    #[inline]
    pub fn redirect(target: &str) -> Response {
        let response = tiny_http::Response::empty(303);
        // TODO: slow \|/
        let response = response.with_header(tiny_http::Header::from_str(&format!("Location: {}", target)).unwrap());

        Response { response: response.boxed() }
    }
}

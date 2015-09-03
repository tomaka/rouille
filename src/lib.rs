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
    server: tiny_http::Server,
}

impl Server {
    pub fn start() -> Server {
        Server {
            server: tiny_http::ServerBuilder::new().build().unwrap(),
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

pub struct Request {
    request: tiny_http::Request,
}

impl Request {
    #[inline]
    pub fn url(&self) -> &str {
        self.request.url()
    }

    #[inline]
    pub fn respond(self, response: Response) {
        self.request.respond(response.response)
    }

    #[inline]
    pub fn respond_to_error(self, err: &RouteError) {
        let response = match err {
            &RouteError::NoRouteFound => tiny_http::Response::empty(404),
            &RouteError::WrongInput => tiny_http::Response::empty(400),
        };

        self.request.respond(response);
    }
}

pub struct Response {
    response: tiny_http::ResponseBox,
}

impl Response {
    /// Builds a `Response` that redirects the user to another URL.
    pub fn redirect(target: &str) -> Response {
        let response = tiny_http::Response::empty(303);
        // TODO: slow \|/
        let response = response.with_header(tiny_http::Header::from_str(&format!("Location: {}", target)).unwrap());

        Response { response: response.boxed() }
    }
}


extern crate tiny_http;

mod input;
mod router;

pub enum RouteError {

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
}

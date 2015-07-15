#![warn(missing_docs)]

extern crate hyper;

use std::net::ToSocketAddrs;

use hyper::server::Listening;
use hyper::server::Server as HyperServer;

pub mod input;
pub mod output;
pub mod route;

/// Starts a server with the given router.
pub fn start<T>(addr: T, router: route::Router) where T: ToSocketAddrs {
    let server = HyperServer::http(addr).unwrap();
    let _ = server.handle(RequestHandler { router: router }).unwrap();
}

struct RequestHandler {
    router: route::Router,
}

impl hyper::server::Handler for RequestHandler {
    fn handle<'a, 'k>(&'a self, request: hyper::server::request::Request<'a, 'k>,
                      response: hyper::server::response::Response<'a, hyper::net::Fresh>)
    {
        for route in self.router.routes.iter() {
            // TODO: 
            match route.handler {    
                route::Handler::Static(_) => unimplemented!(),
                route::Handler::Dynamic(ref handler) => {
                    handler.call(request, response);
                    break;
                },
            }
        }
    }
}

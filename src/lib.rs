#![warn(missing_docs)]

extern crate hyper;
extern crate mustache;
extern crate rustc_serialize;
extern crate term;
extern crate time;

use std::net::ToSocketAddrs;

use hyper::server::Listening;
use hyper::server::Server as HyperServer;

use log::LogProvider;

pub mod input;
pub mod log;
pub mod output;
pub mod route;
pub mod service;

/// Starts a server with the given router.
pub fn start<T>(addr: T, router: route::Router, services: service::StaticServices)
                where T: ToSocketAddrs
{
    let handler = RequestHandler {
        router: router,
        logs: Box::new(log::term::TermLog::new()),
        static_services: services,
    };

    let server = HyperServer::http(addr).unwrap();
    let _ = server.handle(handler).unwrap();
}

struct RequestHandler {
    router: route::Router,
    logs: Box<log::LogProvider + Send + Sync>,
    static_services: service::StaticServices,
}

impl hyper::server::Handler for RequestHandler {
    fn handle<'a, 'k>(&'a self, request: hyper::server::request::Request<'a, 'k>,
                      response: hyper::server::response::Response<'a, hyper::net::Fresh>)
    {
        let time_before = time::precise_time_ns();
        let (method, uri) = (request.method.clone(), request.uri.clone());

        for route in self.router.routes.iter() {
            if !route.matches(&request) {
                continue;
            }

            match route.handler {    
                route::Handler::Static(_) => unimplemented!(),
                route::Handler::Dynamic(ref handler) => {
                    handler.call(request, response, &self.static_services);
                    break;
                },
            }
        }

        let time_after = time::precise_time_ns();
        self.logs.log_request(&method, &uri, time_after - time_before);
    }
}

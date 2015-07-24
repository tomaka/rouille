#![warn(missing_docs)]

extern crate hyper;
extern crate mime;
extern crate mime_guess;
extern crate mustache;
extern crate openssl;
extern crate postgres;
extern crate rustc_serialize;
extern crate term;
extern crate time;

use std::io;
use std::fs;
use std::fs::File;
use std::net::ToSocketAddrs;
use std::path::PathBuf;

use hyper::mime::Mime as HyperMime;
use hyper::mime::TopLevel as HyperMimeTopLevel;
use hyper::mime::Attr as HyperMimeAttr;
use hyper::mime::Value as HyperMimeValue;
use hyper::header::ContentType as HyperContentType;
use hyper::uri::RequestUri as HyperRequestUri;
use hyper::server::Server as HyperServer;

use service::log::LogProvider;

pub mod input;
pub mod output;
pub mod route;
pub mod service;

/// Starts a server with the given router.
///
/// # Parameters
///
/// - `router`: The list of routes that are handled dynamically. You are encouraged to use the
///             `router!` macro to build a router.
///
/// - `static_files`: A path to the directory containing the static files to serve.
///
/// - `services`: Configuration for the various services accessible in the dynamic routes.
///
/// # Example
///
/// ```no_run
/// # #[macro_use] extern crate rouille;
/// # fn main() {
/// # fn handler(_: rouille::input::Ignore) -> rouille::output::PlainTextOutput { panic!() }
/// use rouille::service::StaticServices;
/// use rouille::service::TemplatesCache;
///
/// let router = router! {
///     GET "/" => handler as fn(_) -> _
/// };
///
/// let services = StaticServices {
///     templates: TemplatesCache::new("./templates"),
///     .. Default::default()
/// };
///
/// rouille::start("0.0.0.0:8000", router, "static", services);
/// # }
/// ```
///
pub fn start<T, P>(addr: T, router: route::Router, static_files: P,
                   services: service::StaticServices)
                   where T: ToSocketAddrs, P: Into<PathBuf>
{
    let handler = RequestHandler {
        router: router,
        static_files: static_files.into(),
        static_services: services,
    };

    let server = HyperServer::http(addr).unwrap();
    let _ = server.handle(handler).unwrap();
}

struct RequestHandler {
    router: route::Router,
    static_files: PathBuf,
    static_services: service::StaticServices,
}

impl hyper::server::Handler for RequestHandler {
    fn handle<'a, 'k>(&'a self, request: hyper::server::request::Request<'a, 'k>,
                      response: hyper::server::response::Response<'a, hyper::net::Fresh>)
    {
        let time_before = time::precise_time_ns();
        let (method, uri) = (request.method.clone(), request.uri.clone());

        // handling static files
        if let HyperRequestUri::AbsolutePath(ref url) = request.uri {
            let possible_file = self.static_files.join(&url[1..]);      // TODO: this is a dirty hack to remove the leading `/`

            // FIXME (SECURITY): check with `relative_from` that we're still in `self.static_files`
            //                   once the function is stable

            if fs::metadata(&possible_file).map(|d| d.is_file()).ok().unwrap_or(false) {
                if let Ok(mut file) = File::open(&possible_file) {
                    let mut response = response;

                    let mut mime = mime_guess::guess_mime_type(&possible_file);

                    // adding `charset=utf8` if starting with `text/`
                    if let HyperMime(HyperMimeTopLevel::Text, _, _) = mime {
                        let HyperMime(_, _, ref mut params) = mime;
                        if params.is_empty() { *params = vec![(HyperMimeAttr::Charset, HyperMimeValue::Utf8)]; }
                    }
                    response.headers_mut().set(HyperContentType(mime));

                    if let Ok(mut response) = response.start() {
                        let _ = io::copy(&mut file, &mut response);
                        let _ = response.end();
                    }

                    let time_after = time::precise_time_ns();
                    self.static_services.logs.log_request(&method, &uri, time_after - time_before);
                    return;
                }
            }
        }

        if let Some((route, params)) = self.router.routes.iter()
                                                         .filter_map(|r| {
                                                             r.matches(&request).ok().map(|res| {
                                                                 (r, res)
                                                             })
                                                         })
                                                         .next()
        {
            route.handler.call(request, response, &self.static_services, &params);
        } else {
            println!("No route found!");        // TODO: 
        }

        let time_after = time::precise_time_ns();
        self.static_services.logs.log_request(&method, &uri, time_after - time_before);
    }
}

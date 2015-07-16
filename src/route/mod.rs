use std::path::PathBuf;

use input::Input;
use output::Output;
use service::ServiceAccess;
use service::StaticServices;

use hyper::server::request::Request as HyperRequest;
use hyper::server::response::Response as HyperResponse;
use hyper::method::Method as HyperMethod;

pub struct Route {
    pub url: String,
    pub method: MethodsMask,
    pub handler: Handler,
}

impl Route {
    /// Returns true if this route can handle the given request.
    pub fn matches(&self, request: &HyperRequest) -> bool {
        if !self.method.matches(&request.method) {
            return false;
        }

        true        // FIXME: 
    }
}

/// Describes which methods must be used by the request for a route to be used.
pub struct MethodsMask {
    /// True if the `GET` method matches this mask.
    pub get: bool,
    /// True if the `POST` method matches this mask.
    pub post: bool,
    /// True if the `PUT` method matches this mask.
    pub put: bool,
    /// True if the `DELETE` method matches this mask.
    pub delete: bool,
}

impl MethodsMask {
    /// Parses from a string of the `route!` macro.
    pub fn parse(_: &str) -> MethodsMask {
        // FIXME:
        MethodsMask {
            get: true,
            post: false,
            put: false,
            delete: false,
        }
    }

    /// Returns true if the mask contains the specified method.
    pub fn matches(&self, method: &HyperMethod) -> bool {
        match method {
            &HyperMethod::Get => self.get,
            &HyperMethod::Post => self.post,
            &HyperMethod::Put => self.put,
            &HyperMethod::Delete => self.delete,
            _ => false
        }
    }
}

/// Describes how to handle a route.
pub enum Handler {
    Static(PathBuf),
    Dynamic(Box<DynamicHandler + Send + Sync>),
}

pub struct Router {
    /// List of the routes to try to match. They will be tried in this order.
    pub routes: Vec<Route>,
}

/// Describes types that can process a route.
pub trait DynamicHandler {
    /// Handles a request.
    fn call(&self, HyperRequest, HyperResponse, &StaticServices);
}

impl<I, O> DynamicHandler for fn(I) -> O where I: Input, O: Output {
    fn call(&self, request: HyperRequest, response: HyperResponse, services: &StaticServices) {
        let input = match I::process(request) {
            Ok(i) => i,
            Err(_) => return        // TODO: handle properly
        };

        let output = (*self)(input);
        output.send(response, services);
    }
}

impl<I, O, S1> DynamicHandler for fn(I, S1) -> O
                                  where I: Input, O: Output, S1: for<'s> ServiceAccess<'s>
{
    fn call(&self, request: HyperRequest, response: HyperResponse, services: &StaticServices) {
        let input = match I::process(request) {
            Ok(i) => i,
            Err(_) => return        // TODO: handle properly
        };

        let s1 = S1::load(services);

        let output = (*self)(input, s1);
        output.send(response, services);
    }
}

#[macro_export]
macro_rules! router {
    ($($method:ident $uri:expr => $handler:expr)*) => (
        $crate::route::Router {
            routes: vec![
                $(
                    $crate::route::Route {
                        url: $uri.to_string(),
                        method: $crate::route::MethodsMask::parse(stringify!($method)),
                        handler: $crate::route::Handler::Dynamic(Box::new($handler)
                                             as Box<$crate::route::DynamicHandler + Send + Sync>),
                    }
                ),*
            ]
        }
    );
}

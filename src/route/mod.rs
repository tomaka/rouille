use std::path::PathBuf;
use input::Input;
use output::Output;

use hyper::server::request::Request as HyperRequest;
use hyper::server::response::Response as HyperResponse;
use hyper::method::Method as HyperMethod;

pub struct Route {
    pub url: String,
    pub method: MethodsMask,
    pub handler: Handler,
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
    fn call(&self, HyperRequest, HyperResponse);
}

impl<I, O> DynamicHandler for fn(I) -> O where I: Input, O: Output {
    fn call(&self, request: HyperRequest, response: HyperResponse) {
        let input = I::process(request);
        let output = (*self)(input);
        output.send(response);
    }
}

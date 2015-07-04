//! Convenient wrappers for `hyper::server::Handler`

use hyper::server::{Handler, Request, Response};

use super::Multipart;

/// A container that implements `hyper::server::Handler` which will switch
/// the handler implementation depending on if the incoming request is multipart or not.
///
/// Create an instance with `new()` and pass it to `hyper::server::Server::listen()` where
/// you would normally pass a `Handler` instance, usually a static function.
///
/// A convenient wrapper for `Multipart::from_request()`.
pub struct Switch<H, M> {
    normal: H,
    multipart: M,    
}

impl<H, M> Switch<H, M> where H: Handler, M: MultipartHandler {
    /// Create a new `Switch` instance where  
    /// `normal` handles normal Hyper requests and `multipart` handles Multipart requests
    pub fn new(normal: H, multipart: M) -> Switch<H, M> {
        Switch {
            normal: normal,
            multipart: multipart,
        }
    }
}

impl<H, M> Handler for Switch<H, M> where H: Handler, M: MultipartHandler {
    fn handle(&self, req: Request, res: Response) {
        match Multipart::from_request(req) {
            Ok(multi) => self.multipart.handle_multipart(multi, res),
            Err(req) => self.normal.handle(req, res),
        }
    }
}

/// A trait defining a type that can handle an incoming multipart request.
///
/// Extends to unboxed closures of the type `Fn(Multipart, Response)`, 
/// and subsequently static functions.
///
/// Since `Multipart` implements `Deref<Request>`, you can still access
/// the fields on `Request`, such as `Request::uri` or `Request::headers`.
pub trait MultipartHandler: Send + Sync {
    /// Generate a response from this multipart request.
    fn handle_multipart<'a>(&self, multipart: Multipart<'a>, response: Response);
}

impl<F> MultipartHandler for F where F: for<'a> Fn(Multipart<'a>, Response) + Send + Sync {
    fn handle_multipart<'a>(&self, multipart: Multipart<'a>, response: Response) {
        (*self)(multipart, response);    
    }
}

/// A container for an unboxed closure that implements `hyper::server::Handler`.
///
/// This exists because as of this writing, `Handler` is not automatically implemented for 
/// compatible unboxed closures (though this will likely change).
///
/// No private fields, instantiate directly.
pub struct UnboxedHandler<F> {
    /// The closure to call
    pub f: F,
}

impl<F> Handler for UnboxedHandler<F> where F: Fn(Request, Response) + Send + Sync {
    fn handle(&self, req: Request, res: Response) {
        (self.f)(req, res);
    } 
}




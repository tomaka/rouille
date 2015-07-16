//! Server-side integration with [Hyper](https://github.com/hyperium/hyper).
//! Enabled with the `hyper` feature.
use hyper::net::Fresh;
use hyper::header::ContentType;
use hyper::method::Method;
use hyper::server::{Handler, Request, Response};

use mime::{Mime, TopLevel, SubLevel, Attr, Value};

use super::{Multipart, HttpRequest};

/// A container that implements `hyper::server::Handler` which will switch
/// the handler implementation depending on if the incoming request is multipart or not.
///
/// Create an instance with `new()` and pass it to `hyper::server::Server::listen()` where
/// you would normally pass a `Handler` instance.
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
    fn handle<'a, 'k>(&'a self, req: Request<'a, 'k>, res: Response<'a, Fresh>) {
        match Multipart::from_request(req) {
            Ok(multi) => self.multipart.handle_multipart(multi, res),
            Err(req) => self.normal.handle(req, res),
        }
    }
}

/// A trait defining a type that can handle an incoming multipart request.
///
/// Extends to closures of the type `Fn(Multipart<Request>, Response<Fresh>)`,
/// and subsequently static functions.
pub trait MultipartHandler: Send + Sync {
    /// Generate a response from this multipart request.
    fn handle_multipart<'a, 'k>(&self, 
                                multipart: Multipart<Request<'a, 'k>>, 
                                response: Response<'a, Fresh>);
}

impl<F> MultipartHandler for F 
where F: Fn(Multipart<Request>, Response<Fresh>), F: Send + Sync {
    fn handle_multipart<'a, 'k>(&self, 
                                multipart: Multipart<Request<'a, 'k>>, 
                                response: Response<'a, Fresh>) {
        (*self)(multipart, response);
    }
}

impl<'a, 'b> HttpRequest for Request<'a, 'b> {
    fn is_multipart(&self) -> bool {
        self.method == Method::Post && 
        self.headers.get::<ContentType>().map_or(false, |ct| {
            let ContentType(ref mime) = *ct;

            debug!("Content-Type: {}", mime);

            match *mime {
                Mime(TopLevel::Multipart, SubLevel::FormData, _) => true,
                _ => false,
            }
        })
    }

    fn boundary(&self) -> Option<&str> {
        self.headers.get::<ContentType>().and_then(|ct| {
            let ContentType(ref mime) = *ct;
            let Mime(_, _, ref params) = *mime;

            params.iter().find(|&&(ref name, _)|
                match *name {
                    Attr::Boundary => true,
                    _ => false,
                }
            ).and_then(|&(_, ref val)|
                match *val {
                    Value::Ext(ref val) => Some(&**val),
                    _ => None,
                }
            )
        })
    }
}


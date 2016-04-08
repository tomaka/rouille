//! Server-side integration with [Nickel](http://nickel.rs/) via the `nickel_` feature 
//! (optional, enables `hyper` feature).
//!
//! Not shown here: [`impl HttpRequest for &mut nickel::Request`](../trait.HttpRequest.html#implementors).

use nickel::Request as NickelRequest;

use hyper::server::Request as HyperRequest;

use super::HttpRequest;

impl<'r, 'mw, 'server, D: 'mw> HttpRequest for &'r mut NickelRequest<'mw, 'server, D> {
    type Body = &'r mut HyperRequest<'mw, 'server>;

    fn multipart_boundary(&self) -> Option<&str> {
        self.origin.multipart_boundary()
    }

    fn body(self) -> Self::Body {
        &mut self.origin
    }
}

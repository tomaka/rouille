//! Server-side integration with [Nickel](http://nickel.rs/) via the `nickel_` feature 
//! (optional, enables `hyper` feature).
//!
//! Not shown here: [`impl HttpRequest for &mut nickel::Request`](../trait.HttpRequest.html#implementors).
//!
//! **Note**: in-crate integration for Nickel is deprecated and will be removed in 0.11.0;
//! integration will be provided in the
//! [`multipart-nickel`](https://crates.io/crates/multipart-nickel)
//! crate for the foreseeable future.
#![deprecated(since = "0.10.2", note = "Nickel integration has moved to the `multipart-nickel`
                                        crate; in-crate integration will be removed in 0.11.0")]
use nickel::Request as NickelRequest;

use hyper::server::Request as HyperRequest;

use super::HttpRequest;

#[deprecated(since = "0.10.2", note = "Nickel integration has moved to the `multipart-nickel`
                                       crate; in-crate integration will be removed in 0.11.0")]
impl<'r, 'mw, 'server, D: 'mw> HttpRequest for &'r mut NickelRequest<'mw, 'server, D> {
    type Body = &'r mut HyperRequest<'mw, 'server>;

    fn multipart_boundary(&self) -> Option<&str> {
        self.origin.multipart_boundary()
    }

    fn body(self) -> Self::Body {
        info!("In-crate Nickel integration is deprecated and will be removed in 0.11.0; \
               please consider switching to the `multipart-nickel` crate to avoid breakage.");

        &mut self.origin
    }
}

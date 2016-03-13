//! Trait impl for Nickel, piggybacking on Hyper's integration.

use nickel::Request;

use hyper::server::Request as HyperRequest;

use super::HttpRequest;

impl<'r, 'mw, 'server, D: 'mw> HttpRequest for &'r Request<'mw, 'server, D> {
    type Body = &'r mut HyperRequest<'mw, 'server>;

    fn multipart_boundary(&self) -> Option<&str> {
        self.origin.multipart_boundary()
    }

    fn body(self) -> Self::Body {
        &mut self.origin
    }
}

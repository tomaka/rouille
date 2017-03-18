/// Support for `multipart/form-data` bodies in [Nickel](https://nickel.rs) via the
/// [`multipart`](https://crates.io/crates/multipart) crate.
///
/// ### Why an external crate?
/// Three major reasons led to the decision to move Nickel integration to an external crate.
///
/// 1: The part of Nickel's public API that matters to `multipart` (getting headers and
/// reading the request) uses Hyper's request type; this means that Nickel's integration
/// must necessarily be coupled to Hyper's integration.
///
/// 2: Cargo does not allow specifying two different versions of the same crate in the
/// same manifest, probably for some good reasons but this crate's author has not looked into it.
///
/// 3: Nickel's development moves incredibly slowly; when a new version of Hyper releases, there is
/// always a significant lag before Nickel upgrades,
/// [sometimes several months](https://github.com/nickel-org/nickel.rs/issues/367)--in this case,
/// Hyper was upgraded (in May) two months before the issue was opened (July), but a new version of
/// Nickel was not published until four months later (September).
///
/// This causes problems for `multipart` because `multipart` cannot upgrade Hyper until Nickel does,
/// but its Hyper users often want to upgrade their Hyper as soon as possible.
///
/// In order to provide up-to-date integration for Hyper, it was necessary to move Nickel
/// integration to an external crate so it can be pinned at the version of Hyper that Nickel
/// supports. This allows `multipart` to upgrade Hyper arbitrarily and still keep everyone happy.
///
/// ### Porting from `multipart`'s Integration
///
/// Whereas `multipart` only provided one way to wrap a Nickel request, this crate provides two:
///
/// * To continue using `Multipart::from_request()`, wrap the request in
/// [`Maybe`](struct.Maybe.html):
///
/// ```ignore
/// // Where `req` is `&mut nickel::Request`
/// - Multipart::from_request(req)
/// + use multipart_nickel::Maybe;
/// + Multipart::from_request(Maybe(req))
/// ```
///
/// * Import `multipart_nickel::MultipartBody` and call `.multipart_body()`, which returns
/// `Option` (which better matches the conventions of `nickel::FormBody` and `nickel::JsonBody`):
///
/// ```rust,ignore
/// use multipart_nickel::MultipartBody;
///
/// // Where `req` is `&mut nickel::Request`
/// match req.multipart_body() {
///     Some(multipart) => // handle multipart body
///     None => // handle regular body
/// }
/// ```
extern crate hyper;
extern crate multipart;
extern crate nickel;

use nickel::Request as NickelRequest;

use hyper::server::Request as HyperRequest;

pub use multipart::server as multipart_server;

use multipart_server::{HttpRequest, Multipart};

/// A wrapper for `&mut nickel::Request` which implements `multipart::server::HttpRequest`.
///
/// Necessary because this crate cannot directly provide an impl of `HttpRequest` for
/// `&mut NickelRequest`.
pub struct Maybe<'r, 'mw: 'r, 'server: 'mw, D: 'mw>(pub &'r mut NickelRequest<'mw, 'server, D>);

impl<'r, 'mw: 'r, 'server: 'mw, D: 'mw> HttpRequest for Maybe<'r, 'mw, 'server, D> {
    type Body = &'r mut HyperRequest<'mw, 'server>;

    fn multipart_boundary(&self) -> Option<&str> {
        self.0.origin.multipart_boundary()
    }

    fn body(self) -> Self::Body {
        &mut self.0.origin
    }
}

/// Extension trait for getting the `multipart/form-data` body from `nickel::Request`.
///
/// Implemented for `nickel::Request`.
pub trait MultipartBody<'mw, 'server> {
    /// Get a multipart reader for the request body, if the request is of the right type.
    fn multipart_body(&mut self) -> Option<Multipart<&mut HyperRequest<'mw, 'server>>>;
}

impl<'mw, 'server, D: 'mw> MultipartBody<'mw, 'server> for NickelRequest<'mw, 'server, D> {
    fn multipart_body(&mut self) -> Option<Multipart<&mut HyperRequest<'mw, 'server>>> {
        Multipart::from_request(Maybe(self)).ok()
    }
}


impl<'r, 'mw: 'r, 'server: 'mw, D: 'mw> AsRef<&'r mut NickelRequest<'mw, 'server, D>> for Maybe<'r, 'mw, 'server, D> {
    fn as_ref(&self) -> &&'r mut NickelRequest<'mw, 'server, D> {
        &self.0
    }
}

impl<'r, 'mw: 'r, 'server: 'mw, D: 'mw> AsMut<&'r mut NickelRequest<'mw, 'server, D>> for Maybe<'r, 'mw, 'server, D> {
    fn as_mut(&mut self) -> &mut &'r mut NickelRequest<'mw, 'server, D> {
        &mut self.0
    }
}

impl<'r, 'mw: 'r, 'server: 'mw, D: 'mw> Into<&'r mut NickelRequest<'mw, 'server, D>> for Maybe<'r, 'mw, 'server, D> {
    fn into(self) -> &'r mut NickelRequest<'mw, 'server, D> {
        self.0
    }
}

impl<'r, 'mw: 'r, 'server: 'mw, D: 'mw> From<&'r mut NickelRequest<'mw, 'server, D>> for Maybe<'r, 'mw, 'server, D> {
    fn from(req: &'r mut NickelRequest<'mw, 'server, D>) -> Self {
        Maybe(req)
    }
}


// Copyright (c) 2016 The Rouille developers
// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT
// license <LICENSE-MIT or http://opensource.org/licenses/MIT>,
// at your option. All files in the project carrying such
// notice may not be copied, modified, or distributed except
// according to those terms.

/// Evaluates each parameter until one of them evaluates to something else
/// than `Err(RouteError::NoRouteFound)`.
///
/// This macro supposes that each route returns a `Result<_, RouteError>`.
///
/// # Example
///
/// ```no_run
/// # #[macro_use] extern crate rouille;
/// # fn main() {
/// use rouille::{Request, Response, RouteError};
///
/// fn handle_request_a(_: &Request) -> Result<Response, RouteError> {
/// # panic!()
///    // ...
/// }
///
/// fn handle_request_b(_: &Request) -> Result<Response, RouteError> {
/// # panic!()
///    // ...
/// }
///
/// fn handle_request_c(_: &Request) -> Result<Response, RouteError> {
/// # panic!()
///    // ...
/// }
///
/// # let request = unsafe { ::std::mem::uninitialized() };
/// // First calls `handle_request_a`. If it returns anything else than `NoRouteFound`, then the
/// // `response` will contain the return value.
/// //
/// // Instead if `handle_request_a` returned `NoRouteFound`, then `handle_request_b` is tried.
/// // If `handle_request_b` also returns `NoRouteFound`, then `handle_request_c` is tried.
/// let response = find_route!(
///     handle_request_a(request),
///     handle_request_b(request),
///     handle_request_c(request)
/// );
/// # }
/// ```
///
#[macro_export]
macro_rules! find_route {
    ($($handler:expr),+) => ({
        let mut response = Err($crate::RouteError::NoRouteFound);
        $(
            if let Err($crate::RouteError::NoRouteFound) = response {
                response = $handler;
            }
        )+
        response
    });
}

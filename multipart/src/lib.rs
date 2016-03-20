// Copyright 2016 `multipart` Crate Developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.
//! Client- and server-side abstractions for HTTP `multipart/form-data` requests.
//!
//! Features: 
//! 
//! * `client` (default): Enable the client-side abstractions for multipart requests. If the
//! `hyper` feature is also set, enables integration with the Hyper HTTP client API.
//!
//! * `server` (default): Enable the server-side abstractions for multipart requests. If the
//! `hyper` feature is also set, enables integration with the Hyper HTTP server API.
//!
//! * `hyper` (default): Enable integration with the [Hyper](https://github.com/hyperium/hyper) HTTP library 
//! for client and/or server depending on which other feature flags are set.
//!
//! * `iron`: Enable integration with the [Iron](http://ironframework.io) web application
//! framework. See the [`server::iron`](server/iron/index.html) module for more information.
//!
//! * `tiny_http`: Enable integration with the [`tiny_http`](https://github.com/frewsxcv/tiny-http)
//! crate. See the [`server::tiny_http`](server/tiny_http/index.html) module for more information.
#![warn(missing_docs)]
#[macro_use] extern crate log;
extern crate env_logger;

extern crate mime;
extern crate mime_guess;
extern crate rand;

extern crate tempdir;

#[cfg(feature = "hyper")]
extern crate hyper;

#[cfg(feature = "iron")]
extern crate iron;

#[cfg(feature = "nickel")]
extern crate nickel;

#[cfg(feature = "tiny_http")]
extern crate tiny_http;

use rand::Rng;

/// Chain a series of results together, with or without previous results.
///
/// ```
/// #[macro_use] extern crate multipart;
///
/// fn try_add_one(val: u32) -> Result<u32, u32> {
///     if val < 5 {
///         Ok(val + 1)
///     } else {
///         Err(val)
///     }
/// }
/// 
/// fn main() {
///     let res = chain_result! {
///         try_add_one(1),
///         prev -> try_add_one(prev),
///         prev -> try_add_one(prev),
///         prev -> try_add_one(prev)
///     };
///
///     println!("{:?}", res);
/// }
///
/// ```
#[macro_export]
macro_rules! chain_result {
    ($first_expr:expr, $($try_expr:expr),*) => (
        $first_expr $(.and_then(|_| $try_expr))*
    );
    ($first_expr:expr, $($($arg:ident),+ -> $try_expr:expr),*) => (
        $first_expr $(.and_then(|$($arg),+| $try_expr))*
    );
}

#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod server;

#[cfg(all(test, feature = "client", feature = "server"))]
mod local_test;

fn random_alphanumeric(len: usize) -> String {
    rand::thread_rng().gen_ascii_chars().take(len).collect()
}


// Copyright 2016 `multipart` Crate Developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.
//! Client- and server-side abstractions for HTTP `multipart/form-data` requests.
//!
//! ### Features:
//! This documentation is built with all features enabled.
//!
//! * `client`: The client-side abstractions for generating multipart requests.
//!
//! * `server`: The server-side abstractions for parsing multipart requests.
//!
//! * `mock`: Provides mock implementations of core `client` and `server` traits for debugging
//! or non-standard use.
//!
//! * `hyper`: Integration with the [Hyper](https://github.com/hyperium/hyper) HTTP library
//! for client and/or server depending on which other feature flags are set.
//!
//! * `iron`: Integration with the [Iron](http://ironframework.io) web application
//! framework. See the [`server::iron`](server/iron/index.html) module for more information.
//!
//! * `tiny_http`: Integration with the [`tiny_http`](https://github.com/frewsxcv/tiny-http)
//! crate. See the [`server::tiny_http`](server/tiny_http/index.html) module for more information.
//!
//! **Note**: in-crate integration for Nickel was removed in 0.11.0;
//! integration will be provided in the
//! [`multipart-nickel`](https://crates.io/crates/multipart-nickel)
//! crate for the foreseeable future.
#![cfg_attr(feature="clippy", feature(plugin))]
#![cfg_attr(feature="clippy", plugin(clippy))]
#![cfg_attr(feature="clippy", deny(clippy))]
#![cfg_attr(feature = "bench", feature(test))]
#![deny(missing_docs)]

#[macro_use]
extern crate log;

#[cfg(test)]
extern crate env_logger;

#[cfg_attr(test, macro_use)]
extern crate mime;

extern crate mime_guess;
extern crate rand;
extern crate tempdir;

#[cfg(feature = "server")]
extern crate safemem;

#[cfg(feature = "hyper")]
extern crate hyper;

#[cfg(feature = "iron")]
extern crate iron;

#[cfg(feature = "tiny_http")]
extern crate tiny_http;

#[cfg(any(feature = "mock", test))]
pub mod mock;

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

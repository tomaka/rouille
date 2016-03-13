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
//! * `hyper` (default): Enable integration with the [Hyper](https:://github.com/hyperium/hyper) HTTP library 
//! for client and/or server depending on which other feature flags are set.
#![warn(missing_docs)]
#[macro_use] extern crate log;
extern crate env_logger;

extern crate mime;
extern crate mime_guess;
extern crate rand;

#[cfg(feature = "hyper")]
extern crate hyper;

use rand::Rng;

use std::path::PathBuf;

macro_rules! chain_result {
    ($first_expr:expr, $($try_expr:expr),*) => (
        $first_expr $(.and_then(|_| $try_expr))*
    );
}

#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod server;

#[cfg(all(test, feature = "client", feature = "server"))]
mod local_test;

const DIRNAME_LEN: usize = 12;

fn temp_dir() -> PathBuf {
    random_alphanumeric(DIRNAME_LEN).into()
}

fn random_alphanumeric(len: usize) -> String {
    rand::thread_rng().gen_ascii_chars().take(len).collect()
}


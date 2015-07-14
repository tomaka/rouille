//! Client- and server-side abstractions for HTTP `multipart/form-data` requests.
//!
//! Features:
//! * `hyper`: Enable client- and server-side integration with
//! [Hyper](https:://github.com/hyperium/hyper)
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

macro_rules! try_all {
    ($first_expr:expr, $($try_expr:expr),*) => (
        try!($first_expr $(.and_then(|_| $try_expr))*);
    )
}

macro_rules! chain_result {
    ($first_expr:expr, $($try_expr:expr),*) => (
        $first_expr $(.and_then(|_| $try_expr))*
    )
}

pub mod client;
pub mod server;

const BOUNDARY_LEN: usize = 16;
const DIRNAME_LEN: usize = 12;

fn temp_dir() -> PathBuf {
    random_alphanumeric(DIRNAME_LEN).into()
}

fn random_alphanumeric(len: usize) -> String {
    rand::thread_rng().gen_ascii_chars().take(len).collect()
}

fn gen_boundary() -> String {
    let mut boundary = "--".to_owned();
    boundary.extend(rand::thread_rng().gen_ascii_chars().take(BOUNDARY_LEN));
    boundary
}

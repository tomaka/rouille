#[macro_use] extern crate log;

extern crate mime;
extern crate mime_guess;
extern crate rand;
extern crate rustc_serialize;

#[cfg(feature = "hyper")]
extern crate hyper;

use rand::Rng;

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

fn random_alphanumeric(len: usize) -> String {
    rand::thread_rng().gen_ascii_chars().take(len).collect()
}

fn gen_boundary() -> String {
    let mut boundary = "--".to_owned();
    boundary.extend(rand::thread_rng().gen_ascii_chars().take(BOUNDARY_LEN));
    boundary
}

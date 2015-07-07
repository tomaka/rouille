#[macro_use] extern crate log;

extern crate mime;
extern crate mime_guess;
extern crate rand;
extern crate rustc_serialize;

#[cfg(feature = "hyper")]
extern crate hyper;

use mime::Mime;

use std::borrow::Cow;
use std::fmt;

use std::fs::File;
use std::io;
use std::io::prelude::*;
use std::path::{Path, PathBuf};

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

fn gen_boundary() -> String {
    use rand::Rng;

    "--".chars().chain(rand::thread_rng().gen_ascii_chars().take(BOUNDARY_LEN)).collect()
}

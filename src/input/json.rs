// Copyright (c) 2016 The Rouille developers
// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT
// license <LICENSE-MIT or http://opensource.org/licenses/MIT>,
// at your option. All files in the project carrying such
// notice may not be copied, modified, or distributed except
// according to those terms.

//! Parsing JSON data in the body of a request.
//!
//! Returns an error if the content-type of the request is not JSON, if the JSON is malformed,
//! or if a field is missing or fails to parse.
//!
//! # Example
//!
//! ```
//! extern crate rustc_serialize;
//! # #[macro_use] extern crate rouille;
//! # use rouille::{Request, Response};
//! # fn main() {}
//!
//! fn route_handler(request: &Request) -> Response {
//!     #[derive(RustcDecodable)]
//!     struct Json {
//!         field1: String,
//!         field2: i32,
//!     }
//!
//!     let json: Json = try_or_400!(rouille::input::get_json_input(request));
//!     Response::text(format!("field1's value is {}", json.field1))
//! }
//! ```
//!

use rustc_serialize::Decodable;
use rustc_serialize::json;
use std::io::Error as IoError;
use std::io::Read;
use Request;

/// Error that can happen when parsing the JSON input.
#[derive(Debug)]
pub enum JsonError {
    /// Can't parse the body of the request because it was already extracted.
    BodyAlreadyExtracted,

    /// Wrong content type.
    WrongContentType,

    /// Could not read the body from the request. Also happens if the body is not valid UTF-8.
    IoError(IoError),

    /// Error while parsing.
    ParseError(json::DecoderError),
}

impl From<IoError> for JsonError {
    fn from(err: IoError) -> JsonError {
        JsonError::IoError(err)
    }
}

impl From<json::DecoderError> for JsonError {
    fn from(err: json::DecoderError) -> JsonError {
        JsonError::ParseError(err)
    }
}

/// Attempts to parse the request's body as JSON.
///
/// Returns an error if the content-type of the request is not JSON, or if the JSON is malformed.
///
/// # Example
///
/// ```
/// extern crate rustc_serialize;
/// # #[macro_use] extern crate rouille;
/// # use rouille::{Request, Response};
/// # fn main() {}
///
/// fn route_handler(request: &Request) -> Response {
///     #[derive(RustcDecodable)]
///     struct Json {
///         field1: String,
///         field2: i32,
///     }
/// 
///     let json: Json = try_or_400!(rouille::input::get_json_input(request));
///     Response::text(format!("field1's value is {}", json.field1))
/// }
/// ```
///
pub fn get_json_input<O>(request: &Request) -> Result<O, JsonError> where O: Decodable {
    if let Some(header) = request.header("Content-Type") {
        if !header.starts_with("application/json") {
            return Err(JsonError::WrongContentType);
        }
    } else {
        return Err(JsonError::WrongContentType);
    }

    let content = {
        let mut out = String::new();
        if let Some(mut b) = request.data() {
            try!(b.read_to_string(&mut out));
        } else {
            return Err(JsonError::BodyAlreadyExtracted);
        };
        out
    };

    let data = try!(json::decode(&content));
    Ok(data)
}

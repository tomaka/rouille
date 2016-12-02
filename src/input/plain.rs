// Copyright (c) 2016 The Rouille developers
// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT
// license <LICENSE-MIT or http://opensource.org/licenses/MIT>,
// at your option. All files in the project carrying such
// notice may not be copied, modified, or distributed except
// according to those terms.

use std::io::Error as IoError;
use std::io::Read;
use rustc_serialize::base64::FromBase64;
use Request;

/// Error that can happen when parsing the request body as plain text.
#[derive(Debug)]
pub enum PlainTextError {
    /// Can't parse the body of the request because it was already extracted.
    BodyAlreadyExtracted,

    /// Wrong content type.
    WrongContentType,

    /// Could not read the body from the request. Also happens if the body is not valid UTF-8.
    IoError(IoError),
}

impl From<IoError> for PlainTextError {
    fn from(err: IoError) -> PlainTextError {
        PlainTextError::IoError(err)
    }
}

/// Read plain text data from the body of a request.
///
/// Returns an error if the content-type of the request is not text/plain.
///
/// # Example
///
/// ```
/// # #[macro_use] extern crate rouille;
/// # use rouille::{Request, Response};
/// # fn main() {}
/// fn route_handler(request: &Request) -> Response {
///     let text = try_or_400!(rouille::input::plain_text_body(request));
///     Response::text(format!("you sent: {}", text))
/// }
/// ```
///
pub fn plain_text_body(request: &Request) -> Result<String, PlainTextError> {
    if let Some(header) = request.header("Content-Type") {
        if !header.starts_with("text/plain") {
            return Err(PlainTextError::WrongContentType);
        }
    } else {
        return Err(PlainTextError::WrongContentType);
    }

    let mut out = String::new();
    try!(request.data().unwrap().read_to_string(&mut out));
    Ok(out)
}

#[cfg(test)]
mod test {
    use Request;
    use super::plain_text_body;
    use super::PlainTextError;

    #[test]
    fn ok_content_type() {
        let request = Request::fake_http("GET", "/", vec![
            ("Content-Type".to_owned(), "text/plain".to_owned())
        ], b"test".to_vec());

        match plain_text_body(&request) {
            Ok(ref d) if d == "test" => (),
            _ => panic!()
        }
    }

    #[test]
    fn missing_content_type() {
        let request = Request::fake_http("GET", "/", vec![], Vec::new());

        match plain_text_body(&request) {
            Err(PlainTextError::WrongContentType) => (),
            _ => panic!()
        }
    }

    #[test]
    fn wrong_content_type() {
        let request = Request::fake_http("GET", "/", vec![
            ("Content-Type".to_owned(), "text/html".to_owned())
        ], b"test".to_vec());

        match plain_text_body(&request) {
            Err(PlainTextError::WrongContentType) => (),
            _ => panic!()
        }
    }
}

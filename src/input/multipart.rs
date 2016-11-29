// Copyright (c) 2016 The Rouille developers
// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT
// license <LICENSE-MIT or http://opensource.org/licenses/MIT>,
// at your option. All files in the project carrying such
// notice may not be copied, modified, or distributed except
// according to those terms.

//! Parsing data sent with `multipart/form-data`.
//!
//! > **Note**: You are encouraged to look at [the `post` module](../post/index.html) instead in
//! > order to parse data from HTML forms.

use Request;
use RequestBody;

use std::mem;

use multipart::server::Multipart as InnerMultipart;

// TODO: provide wrappers around these
pub use multipart::server::MultipartField;
pub use multipart::server::MultipartData;
pub use multipart::server::MultipartFile;

/// Error that can happen when decoding multipart data.
#[derive(Clone, Debug)]
pub enum MultipartError {
    /// The `Content-Type` header of the request indicates that it doesn't contain multipart data
    /// or is invalid.
    WrongContentType,

    /// Can't parse the body of the request because it was already extracted.
    BodyAlreadyExtracted,
}

/// Attempts to decode the content of the request as `multipart/form-data` data.
pub fn get_multipart_input(request: &Request) -> Result<Multipart, MultipartError> {
    let boundary = match multipart_boundary(request) {
        Some(b) => b,
        None => return Err(MultipartError::WrongContentType)
    };

    let request_body = if let Some(body) = request.data() {
        body
    } else {
        return Err(MultipartError::BodyAlreadyExtracted);
    };

    Ok(Multipart {
        inner: InnerMultipart::with_body(request_body, boundary)
    })
}

/// Allows you to inspect the content of the multipart input of a request.
pub struct Multipart<'a> {
    inner: InnerMultipart<RequestBody<'a>>
}

impl<'a> Multipart<'a> {
    pub fn next(&mut self) -> Option<MultipartField<RequestBody<'a>>> {
        match self.inner.read_entry() {
            Ok(e) => e,
            _ => return None
        }
    }
}

fn multipart_boundary(request: &Request) -> Option<String> {
    const BOUNDARY: &'static str = "boundary=";

    let content_type = match request.header("Content-Type") {
        None => return None,
        Some(c) => c
    };

    let start = match content_type.find(BOUNDARY) {
        Some(pos) => pos + BOUNDARY.len(),
        None => return None
    };

    let end = content_type[start..].find(';').map_or(content_type.len(), |end| start + end);
    Some(content_type[start .. end].to_owned())
}


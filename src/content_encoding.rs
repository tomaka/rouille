// Copyright (c) 2016 The Rouille developers
// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT
// license <LICENSE-MIT or http://opensource.org/licenses/MIT>,
// at your option. All files in the project carrying such
// notice may not be copied, modified, or distributed except
// according to those terms.

use std::str;
use Request;
use Response;

// The AsciiExt import is needed for Rust older than 1.23.0. These two lines can
// be removed when supporting older Rust is no longer needed.
#[allow(unused_imports)]
use std::ascii::AsciiExt;

/// Applies content encoding to the response.
///
/// Analyzes the `Accept-Encoding` header of the request. If one of the encodings is recognized and
/// supported by rouille, it adds a `Content-Encoding` header to the `Response` and encodes its
/// body.
///
/// If the response already has a `Content-Encoding` header, this function is a no-op.
/// If the response has a `Content-Type` header that isn't textual content, this function is a
/// no-op.
///
/// The gzip encoding is supported only if you enable the `gzip` feature of rouille (which is
/// enabled by default).
///
/// # Example
///
/// ```rust
/// use rouille::content_encoding;
/// use rouille::Request;
/// use rouille::Response;
///
/// fn handle(request: &Request) -> Response {
///     content_encoding::apply(request, Response::text("hello world"))
/// }
/// ```
pub fn apply(request: &Request, response: Response) -> Response {
    // Only text should be encoded. Otherwise just return.
    if !response_is_text(&response) {
        return response;
    }

    // If any of the response's headers is equal to `Content-Encoding`, ignore the function
    // call and return immediately.
    if response.headers.iter().any(|&(ref key, _)| key.eq_ignore_ascii_case("Content-Encoding")) {
        return response;
    }

    // Put the response in an Option for later.
    let mut response = Some(response);

    // Now let's get the list of content encodings accepted by the request.
    // The list should be ordered from the most desired to the list desired.
    // TODO: use input::priority_header_preferred instead
    for encoding in accepted_content_encodings(request) {
        // Try the brotli encoding.
        if brotli(encoding, &mut response) {
            return response.take().unwrap();
        }

        // Try the gzip encoding.
        if gzip(encoding, &mut response) {
            return response.take().unwrap();
        }

        // The identity encoding is always supported.
        if encoding.eq_ignore_ascii_case("identity") {
            return response.take().unwrap();
        }
    }

    // No encoding accepted, don't do anything.
    response.take().unwrap()
}

// Returns true if the Content-Type of the response is a type that should be encoded.
// Since encoding is purely an optimisation, it's not a problem if the function sometimes has
// false positives or false negatives.
fn response_is_text(response: &Response) -> bool {
    response.headers.iter().any(|&(ref key, ref value)| {
        if !key.eq_ignore_ascii_case("Content-Type") {
            return false;
        }

        // TODO: perform case-insensitive comparison
        value.starts_with("text/") || value.contains("javascript") || value.contains("json") ||
            value.contains("xml") || value.contains("font")
    })
}

/// Returns an iterator of the list of content encodings accepted by the request.
///
/// # Example
///
/// ```
/// use rouille::{Request, Response};
/// use rouille::content_encoding;
///
/// fn handle(request: &Request) -> Response {
///     for encoding in content_encoding::accepted_content_encodings(request) {
///         // ...
///     }
///
///     // ...
/// # panic!()
/// }
/// ```
pub fn accepted_content_encodings(request: &Request) -> AcceptedContentEncodingsIter {
    let elems = request.header("Accept-Encoding").unwrap_or("").split(',');
    AcceptedContentEncodingsIter { elements: elems }
}

/// Iterator to the list of content encodings accepted by a request.
pub struct AcceptedContentEncodingsIter<'a> {
    elements: str::Split<'a, char>
}

impl<'a> Iterator for AcceptedContentEncodingsIter<'a> {
    type Item = &'a str;

    #[inline]
    fn next(&mut self) -> Option<&'a str> {
        loop {
            match self.elements.next() {
                None => return None,
                Some(e) => {
                    let e = e.trim();
                    if !e.is_empty() {
                        return Some(e);
                    }
                }
            }
        }
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        let (_, max) = self.elements.size_hint();
        (0, max)
    }
}

#[cfg(feature = "gzip")]
fn gzip(e: &str, response: &mut Option<Response>) -> bool {
    use ResponseBody;
    use std::mem;
    use std::io;
    use deflate::deflate_bytes_gzip;

    if !e.eq_ignore_ascii_case("gzip") {
        return false;
    }

    let response = response.as_mut().unwrap();
    response.headers.push(("Content-Encoding".into(), "gzip".into()));
    let previous_body = mem::replace(&mut response.data, ResponseBody::empty());
    let (mut raw_data, size) = previous_body.into_reader_and_size();
    let mut src = match size {
        Some(size) => Vec::with_capacity(size),
        None => Vec::new(),
    };
    io::copy(&mut raw_data, &mut src).expect("Failed reading response body while gzipping");
    let zipped = deflate_bytes_gzip(&src);
    response.data = ResponseBody::from_data(zipped);
    true
}

#[cfg(not(feature = "gzip"))]
#[inline]
fn gzip(e: &str, response: &mut Option<Response>) -> bool {
    false
}

#[cfg(feature = "brotli")]
fn brotli(e: &str, response: &mut Option<Response>) -> bool {
    use ResponseBody;
    use std::mem;
    use brotli2::read::BrotliEncoder;

    if !e.eq_ignore_ascii_case("br") {
        return false;
    }

    let response = response.as_mut().unwrap();
    response.headers.push(("Content-Encoding".into(), "br".into()));
    let previous_body = mem::replace(&mut response.data, ResponseBody::empty());
    let (raw_data, _) = previous_body.into_reader_and_size();
    response.data = ResponseBody::from_reader(BrotliEncoder::new(raw_data, 6));
    true
}

#[cfg(not(feature = "brotli"))]
#[inline]
fn brotli(e: &str, response: &mut Option<Response>) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use Request;
    use content_encoding;

    #[test]
    fn no_req_encodings() {
        let request = Request::fake_http("GET", "/", vec![], vec![]);
        assert_eq!(content_encoding::accepted_content_encodings(&request).count(), 0);
    }

    #[test]
    fn empty_req_encodings() {
        let request = {
            let h = vec![("Accept-Encoding".to_owned(), "".to_owned())];
            Request::fake_http("GET", "/", h, vec![])
        };

        assert_eq!(content_encoding::accepted_content_encodings(&request).count(), 0);
    }

    #[test]
    fn one_req_encoding() {
        let request = {
            let h = vec![("Accept-Encoding".to_owned(), "foo".to_owned())];
            Request::fake_http("GET", "/", h, vec![])
        };

        let mut list = content_encoding::accepted_content_encodings(&request);
        assert_eq!(list.next().unwrap(), "foo");
        assert_eq!(list.next(), None);
    }

    #[test]
    fn multi_req_encoding() {
        let request = {
            let h = vec![("Accept-Encoding".to_owned(), "foo, bar".to_owned())];
            Request::fake_http("GET", "/", h, vec![])
        };

        let mut list = content_encoding::accepted_content_encodings(&request);
        assert_eq!(list.next().unwrap(), "foo");
        assert_eq!(list.next().unwrap(), "bar");
        assert_eq!(list.next(), None);
    }

    // TODO: more tests for encoding stuff
}

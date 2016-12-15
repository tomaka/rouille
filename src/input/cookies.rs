// Copyright (c) 2016 The Rouille developers
// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT
// license <LICENSE-MIT or http://opensource.org/licenses/MIT>,
// at your option. All files in the project carrying such
// notice may not be copied, modified, or distributed except
// according to those terms.

//! Analyze the request's headers and body.
//! 
//! This module provides functions and sub-modules that allow you to easily analyze or parse the
//! request's headers and body.
//! 
//! - In order to parse JSON, see [the `json` module](json/input.html).
//! - In order to parse input from HTML forms, see [the `post` module](post/input.html).
//! - In order to read a plain text body, see
//!   [the `plain_text_body` function](fn.plain_text_body.html).

use Request;

/// Attempts to parse the list of cookies from the request.
///
/// Returns a pair of `(key, value)`. If the header is missing or malformed, an empty
/// `Vec` is returned.
// TODO: should an error be returned if the header is malformed?
// TODO: be less tolerent to what is accepted?
pub fn cookies(request: &Request) -> Vec<(String, String)> {
    let header = match request.header("Cookie") {
        None => return Vec::new(),
        Some(h) => h,
    };

    header
        .split(|c| c == ';')
        .filter_map(|cookie| {
            let mut splits = cookie.splitn(2, |c| c == '=');
            let key = match splits.next() { None => return None, Some(v) => v };
            let value = match splits.next() { None => return None, Some(v) => v };

            let key = key.trim().to_owned();
            let value = value.trim().trim_matches(|c| c == '"').to_owned();

            Some((key, value))
        })
        .collect()
}

#[cfg(test)]
mod test {
    use Request;
    use super::cookies;

    #[test]
    fn cookies_ok() {
        let request = Request::fake_http("GET", "/",
                                         vec![("Cookie".to_owned(),
                                               "a=b; hello=world".to_owned())],
                                         Vec::new());

        assert_eq!(cookies(&request), vec![
            ("a".to_owned(), "b".to_owned()),
            ("hello".to_owned(), "world".to_owned())
        ]);
    }
}

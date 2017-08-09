// Copyright (c) 2017 The Rouille developers
// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT
// license <LICENSE-MIT or http://opensource.org/licenses/MIT>,
// at your option. All files in the project carrying such
// notice may not be copied, modified, or distributed except
// according to those terms.

use atoi::atoi;
use std::ascii::AsciiExt;
use std::mem;

pub struct RequestBodyAnalyzer {
    remaining_content_length: u64,
}

impl RequestBodyAnalyzer {
    pub fn new<'a, I>(headers: I) -> RequestBodyAnalyzer
        where I: Iterator<Item = (&'a str, &'a str)>        // TODO: should be [u8] eventually
    {
        let mut content_length = None;
        for (header, value) in headers {
            if header.eq_ignore_ascii_case("Content-Length") {
                content_length = atoi(value.as_bytes());
            }
        }

        RequestBodyAnalyzer {
            remaining_content_length: content_length.unwrap_or(0),
        }
    }

    /// Processes some data.
    pub fn feed(&mut self, data: &mut [u8]) -> FeedOutcome {
        // The most common case is a request without any data.
        if self.remaining_content_length == 0 {
            return FeedOutcome {
                body_data: 0,
                finished: true,
            };
        }

        if (data.len() as u64) < self.remaining_content_length {
            self.remaining_content_length -= data.len() as u64;
            return FeedOutcome {
                body_data: data.len(),
                finished: self.remaining_content_length == 0,
            };
        }

        FeedOutcome {
            body_data: mem::replace(&mut self.remaining_content_length, 0) as usize,
            finished: true,
        }
    }
}

pub struct FeedOutcome {
    /// Number of bytes in `data` that contain the body of the request.
    pub body_data: usize,
    /// True if the request is finished. Calling `feed` again would return a `FeedOutcome` with a
    /// `body_data` of 0.
    pub finished: bool,
}

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
use std::cmp;
use std::mem;

pub struct RequestBodyAnalyzer {
    inner: RequestBodyAnalyzerInner,
}

enum RequestBodyAnalyzerInner {
    ContentLength {
        // Remaining body length.
        remaining_content_length: u64,
    },
    ChunkedTransferEncoding {
        // Remaining size of the chunk being read. `None` if we are not in a chunk.
        remaining_chunk_size: Option<usize>,
    },
    EndOfStream,
}

impl RequestBodyAnalyzer {
    /// Reads the request's headers to determine how the body will need to be handled.
    pub fn new<'a, I>(headers: I) -> RequestBodyAnalyzer
        where I: Iterator<Item = (&'a str, &'a str)>        // TODO: should be [u8] eventually
    {
        let mut content_length = None;
        let mut chunked = false;

        for (header, value) in headers {
            if header.eq_ignore_ascii_case("Transfer-Encoding") {
                if value.eq_ignore_ascii_case("chunked") {
                    chunked = true;
                }
            }
            if header.eq_ignore_ascii_case("Content-Length") {
                content_length = atoi(value.as_bytes());
            }
        }

        RequestBodyAnalyzer {
            inner: match (content_length, chunked) {
                (_, true) => RequestBodyAnalyzerInner::ChunkedTransferEncoding {
                    remaining_chunk_size: None,
                },
                (Some(len), _) => RequestBodyAnalyzerInner::ContentLength {
                    remaining_content_length: len,
                },
                _ => RequestBodyAnalyzerInner::EndOfStream,     // TODO: /!\
            },
        }
    }

    /// Processes some data. Call this method with a slice containing data received by the socket.
    /// This method will "decode" them in place. The decoding always takes less space than the
    /// input, so there's no buffering of any sort.
    pub fn feed(&mut self, data: &mut [u8]) -> FeedOutcome {
        match self.inner {
            RequestBodyAnalyzerInner::ContentLength { ref mut remaining_content_length } => {
                if (data.len() as u64) < *remaining_content_length {
                    *remaining_content_length -= data.len() as u64;
                    FeedOutcome {
                        body_data: data.len(),
                        unused_trailing: 0,
                        finished: *remaining_content_length == 0,
                    }

                } else {
                    FeedOutcome {
                        body_data: mem::replace(&mut *remaining_content_length, 0) as usize,
                        unused_trailing: 0,
                        finished: true,
                    }
                }
            },

            RequestBodyAnalyzerInner::ChunkedTransferEncoding { ref mut remaining_chunk_size } => {
                // `out_body_data` contains the number of bytes from the start of `data` that are
                // already final.
                //
                // `out_unused_trailing` contains the number of bytes after `out_body_data` that
                // are garbage.
                //
                // Therefore at any point during this algorithm,
                // `out_body_data + out_unused_trailing` is the offset of the next byte of input.
                //
                // Incrementing `out_unused_trailing` means that we skip bytes from the input.
                let mut out_body_data = 0;
                let mut out_unused_trailing = 0;

                loop {
                    if remaining_chunk_size.is_none() {
                        match try_read_chunk_size(&data[out_body_data + out_unused_trailing..]) {
                            Some((skip, chunk_size)) => {
                                *remaining_chunk_size = Some(chunk_size);
                                debug_assert_ne!(skip, 0);
                                out_unused_trailing += skip;
                            },
                            None => return FeedOutcome {
                                body_data: out_body_data,
                                unused_trailing: out_unused_trailing,
                                finished: false,
                            },
                        }
                    }

                    if *remaining_chunk_size == Some(0) {
                        return FeedOutcome {
                            body_data: out_body_data,
                            unused_trailing: out_unused_trailing,
                            finished: true,
                        }
                    }

                    let copy_len = cmp::min(data.len() - out_body_data - out_unused_trailing,
                                            remaining_chunk_size.unwrap());
                    if out_unused_trailing != 0 {
                        for n in 0 .. copy_len {
                            data[out_body_data + n] = data[out_body_data + out_unused_trailing + n];
                        }
                    }
                    out_body_data += copy_len;
                    *remaining_chunk_size.as_mut().unwrap() -= copy_len;
                    if *remaining_chunk_size == Some(0) {
                        *remaining_chunk_size = None;
                    }
                }
            },

            RequestBodyAnalyzerInner::EndOfStream => {
                FeedOutcome {
                    body_data: data.len(),
                    unused_trailing: 0,
                    finished: false,
                }
            },
        }
    }
}

/// Result of the `feed` method.
pub struct FeedOutcome {
    /// Number of bytes from the start of `data` that contain the body of the request. If
    /// `finished` is true, then any further byte is part of the next request. If `finished` is
    /// false, then any further byte is still part of this request but hasn't been decoded yet.
    pub body_data: usize,

    /// Number of bytes following `body_data` that are irrelevant and that should be discarded.
    pub unused_trailing: usize,

    /// True if the request is finished. Calling `feed` again would return a `FeedOutcome` with a
    /// `body_data` of 0.
    pub finished: bool,
}

// Tries to read a chunk size from `data`. Returns `None` if not enough data.
// Returns the number of bytes that make up the chunk size, and the chunk size value.
fn try_read_chunk_size(data: &[u8]) -> Option<(usize, usize)> {
    let crlf_pos = match data.windows(2).position(|n| n == b"\r\n") {
        Some(p) => p,
        None => return None,
    };

    let chunk_size = match atoi(&data[..crlf_pos]) {
        Some(s) => s,
        None => return None,        // TODO: error instead
    };

    Some((crlf_pos + 2, chunk_size))
}

#[cfg(test)]
mod tests {
    use super::RequestBodyAnalyzer;

    #[test]
    fn chunked_decode() {
        let mut analyzer = {
            let headers = vec![("Transfer-Encoding", "chunked")];
            RequestBodyAnalyzer::new(headers.into_iter())
        };

        let mut buffer = b"6\r\nhello 5\r\nworld0\r\n".to_vec();
        let outcome = analyzer.feed(&mut buffer);

        assert_eq!(outcome.body_data, 11);
        assert_eq!(outcome.unused_trailing, 20 - 11);
        assert!(outcome.finished);
        assert_eq!(&buffer[..11], &b"hello world"[..]);
    }
}

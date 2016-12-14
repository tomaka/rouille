// Copyright 2016 `multipart` Crate Developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.
//!
use std::io::{self, Read, Write};
use std::fmt;

use rand::{self, Rng, ThreadRng};

#[derive(Default, Debug)]
pub struct ClientRequest {
    boundary: Option<String>,
    content_len: Option<u64>,
}

#[cfg(feature = "client")]
impl ::client::HttpRequest for ClientRequest {
    type Stream = HttpBuffer;
    type Error = io::Error;

    fn apply_headers(&mut self, boundary: &str, content_len: Option<u64>) -> bool {
        self.boundary = Some(boundary.into());
        self.content_len = content_len;
        true
    }

    /// ##Panics
    /// If `apply_headers()` was not called.
    fn open_stream(self) -> Result<HttpBuffer, io::Error> {
        debug!("MockClientRequest::open_stream called! {:?}", self);
        let boundary = self.boundary.expect("HttpRequest::set_headers() was not called!");

        Ok(HttpBuffer::new_empty(boundary, self.content_len))
    }
}

pub struct HttpBuffer {
    pub buf: Vec<u8>,
    pub boundary: String,
    pub content_len: Option<u64>,
    rng: ThreadRng,
}

impl HttpBuffer {
    pub fn new_empty(boundary: String, content_len: Option<u64>) -> HttpBuffer {
        Self::with_buf(Vec::new(), boundary, content_len)
    }

    pub fn with_buf(buf: Vec<u8>, boundary: String, content_len: Option<u64>) -> Self {
        HttpBuffer {
            buf: buf,
            boundary: boundary,
            content_len: content_len,
            rng: rand::thread_rng()
        }
    }

    pub fn for_server(&self) -> ServerRequest {
        ServerRequest {
            data: &self.buf,
            boundary: &self.boundary,
            content_len: self.content_len,
            rng: rand::thread_rng(),
        }
    }
}

impl Write for HttpBuffer {
    fn write(&mut self, out: &[u8]) -> io::Result<usize> {
        if out.len() == 0 {
            debug!("Passed a zero-sized buffer.");
            return Ok(0);
        }

        // Simulate the randomness of a network connection by not always reading everything
        let len = self.rng.gen_range(1, out.len() + 1);

        self.buf.write(&out[..len])
    }

    fn flush(&mut self) -> io::Result<()> {
        self.buf.flush()
    }
}

#[cfg(feature = "client")]
impl ::client::HttpStream for HttpBuffer {
    type Request = ClientRequest;
    type Response = HttpBuffer;
    type Error = io::Error;

    fn finish(self) -> Result<Self, io::Error> { Ok(self) }
}

impl fmt::Debug for HttpBuffer {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("multipart::mock::HttpBuffer")
            .field("buf", &self.buf)
            .field("boundary", &self.boundary)
            .field("content_len", &self.content_len)
            .finish()
    }
}

pub struct ServerRequest<'a> {
    pub data: &'a [u8],
    pub boundary: &'a str,
    pub content_len: Option<u64>,
    rng: ThreadRng,
}

impl<'a> Read for ServerRequest<'a> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.len() == 0 {
            debug!("Passed a zero-sized buffer.");
            return Ok(0);
        }

        // Simulate the randomness of a network connection by not always reading everything
        let len = self.rng.gen_range(1, out.len() + 1);
        self.data.read(&mut out[..len])
    }
}

#[cfg(feature = "server")]
impl<'a> ::server::HttpRequest for ServerRequest<'a> {
    type Body = Self;

    fn multipart_boundary(&self) -> Option<&str> { Some(&self.boundary) }

    fn body(self) -> Self::Body {
        self
    }
}
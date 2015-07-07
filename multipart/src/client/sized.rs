//! Sized/buffered wrapper around `HttpRequest`.

use client::{HttpRequest, HttpStream};

use std::io;
use std::io::prelude::*;

/// A wrapper around `HttpRequest` that writes the multipart data to an in-memory buffer so its
/// size can be calculated and set in the request as the `Content-Length` header.
pub struct SizedRequest<R> {
    inner: R,
    buffer: Vec<u8>,
    boundary: String,
}

impl<R: HttpRequest> SizedRequest<R> {
    #[doc(hidden)]
    pub fn from_request(req: R) -> SizedRequest<R> {
        SizedRequest {
            inner: req,
            buffer: Vec::new(),
            boundary: String::new(),
        }
    }
}

impl<R> Write for SizedRequest<R> {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.buffer.write(data)
    }

    fn flush(&mut self) -> io::Result<()> { Ok(()) }
}

impl<R: HttpRequest> HttpRequest for SizedRequest<R> 
where <R::Stream as HttpStream>::Error: From<R::Error> {
    type Stream = Self;
    type Error = R::Error;

    /// `SizedRequest` ignores `_content_len` because it sets its own later.
    fn apply_headers(&mut self, boundary: &str, _content_len: Option<usize>) {
        self.boundary.clear();
        self.boundary.push_str(boundary);
    }

    fn open_stream(mut self) -> Result<Self, Self::Error> {
        self.buffer.clear();
        Ok(self)
    }
}

impl<R: HttpRequest> HttpStream for SizedRequest<R> 
where <R::Stream as HttpStream>::Error: From<R::Error> { 
    type Request = R;
    type Response = <<R as HttpRequest>::Stream as HttpStream>::Response;
    type Error = <<R as HttpRequest>::Stream as HttpStream>::Error;

    fn finish(mut self) -> Result<Self::Response, Self::Error>  {
        let content_len = self.buffer.len();
        self.inner.apply_headers(&self.boundary, Some(content_len));

        let mut req = try!(self.inner.open_stream());
        try!(io::copy(&mut &self.buffer[..], &mut req));
        req.finish().into()
    }
}

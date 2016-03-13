// Copyright 2016 `multipart` Crate Developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.
//! The client-side abstraction for multipart requests. Enabled with the `client` feature (on by
//! default).
//!
//! Use this when sending POST requests with files to a server.
use mime::Mime;

use std::fs::File;
use std::io;
use std::io::prelude::*;

use std::path::Path;

#[cfg(feature = "hyper")]
pub mod hyper;

mod sized;

pub use self::sized::SizedRequest;

const BOUNDARY_LEN: usize = 16;


/// The entry point of the client-side multipart API.
///
/// Though they perform I/O, the `.write_*()` methods do not return `io::Result<_>` in order to
/// facilitate method chaining. Upon the first error, all subsequent API calls will be no-ops until
/// `.send()` is called, at which point the error will be reported.
pub struct Multipart<S: HttpStream> {
    stream: S,
    boundary: String,
    last_err: Option<S::Error>,
    data_written: bool,
}

impl Multipart<io::Sink> {
    /// Create a new `Multipart` to wrap a request.
    ///
    /// ## Returns Error
    /// If `req.open_stream()` returns an error.
    pub fn from_request<R: HttpRequest>(mut req: R) -> Result<Multipart<R::Stream>, R::Error> {
        let boundary = ::random_alphanumeric(BOUNDARY_LEN);
        req.apply_headers(&boundary, None);

        let stream = try!(req.open_stream());

        Ok(Multipart {
            stream: stream,
            boundary: boundary,
            last_err: None,
            data_written: false,
        })
    }
}

impl<S: HttpStream> Multipart<S> {
    /// Get a reference to the last error returned from writing to the HTTP stream, if any.
    pub fn last_err(&self) -> Option<&S::Error> {
        self.last_err.as_ref()
    }

    /// Remove and return the last error to occur, allowing subsequent API calls to proceed
    /// normally.
    ///
    /// ##Warning
    /// If an error occurred during a write, the request body may be corrupt.
    pub fn take_err(&mut self) -> Option<S::Error> {
        self.last_err.take()
    }

    /// Write a text field to this multipart request.
    /// `name` and `val` can be either owned `String` or `&str`.
    ///
    /// ##Errors
    /// If something went wrong with the HTTP stream.
    pub fn write_text<N: AsRef<str>, V: AsRef<str>>(&mut self, name: N, val: V) -> &mut Self {
        if self.last_err.is_none() {
            self.last_err = chain_result! {
                self.write_field_headers(name.as_ref(), None, None),
                self.stream.write_all(val.as_ref().as_bytes())
            }.err().map(|err| err.into())
        }

        self
    }
    
    /// Open a file pointed to by `path` and write its contents to the multipart request, 
    /// supplying its filename and guessing its `Content-Type` from its extension.
    ///
    /// If you want to set these values manually, or use another type that implements `Read`, 
    /// use `.write_stream()`.
    ///
    /// `name` can be either `String` or `&str`, and `path` can be `PathBuf` or `&Path`.
    ///
    /// ##Errors
    /// If there was a problem opening the file (was a directory or didn't exist),
    /// or if something went wrong with the HTTP stream.
    pub fn write_file<N: AsRef<str>, P: AsRef<Path>>(&mut self, name: N, path: P) -> &mut Self {
        if self.last_err.is_none() {     
            let path = path.as_ref();

            self.last_err = chain_result! {
                { // New borrow scope so we can reborrow `file` after
                    let content_type = ::mime_guess::guess_mime_type(path);
                    let filename = path.file_name().and_then(|filename| filename.to_str());
                    self.write_field_headers(name.as_ref(), filename, Some(content_type))
                },
                File::open(path).and_then(|ref mut file| io::copy(file, &mut self.stream))
            }.err().map(|err| err.into());
        }

        self
    }

    /// Write a byte stream to the multipart request as a file field, supplying `filename` if given,
    /// and `content_type` if given or `"application/octet-stream"` if not.
    ///
    /// `name` can be either `String` or `&str`, and `read` can take the `Read` by-value or
    /// with an `&mut` borrow.
    ///
    /// ##Warning
    /// The given `Read` **must** be able to read to EOF (end of file/no more data), meaning
    /// `Read::read()` returns `Ok(0)`. If it never returns EOF it will be read to infinity 
    /// and the request will never be completed.
    ///
    /// When using `SizedRequest` this also can cause out-of-control memory usage as the
    /// multipart data has to be written to an in-memory buffer so its size can be calculated.
    ///
    /// Use `Read::take()` if you wish to send data from a `Read` 
    /// that will never return EOF otherwise.
    ///
    /// ##Errors
    /// If the reader returned an error, or if something went wrong with the HTTP stream.
    // RFC: How to format this declaration?
    pub fn write_stream<N: AsRef<str>, St: Read>(
        &mut self, name: N, read: &mut St, filename: Option<&str>, content_type: Option<Mime>
    ) -> &mut Self {
        if self.last_err.is_none() {
            let content_type = content_type.unwrap_or_else(::mime_guess::octet_stream);

            self.last_err = chain_result! {
                self.write_field_headers(name.as_ref(), filename, Some(content_type)),
                io::copy(read, &mut self.stream)
            }.err().map(|err| err.into());
        }

        self
    } 

    fn write_field_headers(&mut self, name: &str, filename: Option<&str>, content_type: Option<Mime>) 
    -> io::Result<()> {
        self.data_written = true;

        chain_result! {
            // Write the first boundary, or the boundary for the previous field.
            write!(self.stream, "\r\n--{}\r\n", self.boundary),
            write!(self.stream, "Content-Disposition: form-data; name=\"{}\"", name),
            filename.map(|filename| write!(self.stream, "; filename=\"{}\"", filename))
                .unwrap_or(Ok(())),
            content_type.map(|content_type| write!(self.stream, "\r\nContent-Type: {}", content_type))
                .unwrap_or(Ok(())),
            self.stream.write_all(b"\r\n\r\n")
        }
    }

    /// Finalize the request and return the response from the server, or the last error if set.
    pub fn send(mut self) -> Result<S::Response, S::Error> {
        match self.last_err {
            None => {
                if self.data_written {
                    // Write two hyphens after the last boundary occurrence.
                    try!(write!(self.stream, "\r\n--{}--", self.boundary));
                }
                
                self.stream.finish()
            },
            Some(err) => Err(err),
        }
    }    
}

impl<R: HttpRequest> Multipart<SizedRequest<R>>
where <R::Stream as HttpStream>::Error: From<R::Error> {
    /// Create a new `Multipart` using the `SizedRequest` wrapper around `req`.
    pub fn from_request_sized(req: R) -> Result<Self, R::Error> {
        Multipart::from_request(SizedRequest::from_request(req))
    }
}

/// A trait describing an HTTP request that can be used to send multipart data.
pub trait HttpRequest {
    /// The HTTP stream type that can be opend by this request, to which the multipart data will be
    /// written.
    type Stream: HttpStream;
    /// The error type for this request. 
    /// Must be compatible with `io::Error` as well as `Self::HttpStream::Error`
    type Error: From<io::Error> + Into<<Self::Stream as HttpStream>::Error>;

    /// Set the `Content-Type` header to `multipart/form-data` and supply the `boundary` value.
    /// If `content_len` is given, set the `Content-Length` header to its value.
    /// 
    /// Return `true` if any and all sanity checks passed and the stream is ready to be opened, 
    /// or `false` otherwise.
    fn apply_headers(&mut self, boundary: &str, content_len: Option<u64>) -> bool;

    /// Open the request stream and return it or any error otherwise. 
    fn open_stream(self) -> Result<Self::Stream, Self::Error>;
}

/// A trait describing an open HTTP stream that can be written to.
pub trait HttpStream: Write {
    /// The request type that opened this stream.
    type Request: HttpRequest;
    /// The response type that will be returned after the request is completed.
    type Response;
    /// The error type for this stream.
    /// Must be compatible with `io::Error` as well as `Self::Request::Error`.
    type Error: From<io::Error> + From<<Self::Request as HttpRequest>::Error>; 

    /// Finalize and close the stream and return the response object, or any error otherwise.
    fn finish(self) -> Result<Self::Response, Self::Error>;
}

impl HttpRequest for () {
    type Stream = io::Sink;
    type Error = io::Error;

    fn apply_headers(&mut self, _: &str, _: Option<u64>) -> bool { true }
    fn open_stream(self) -> Result<Self::Stream, Self::Error> { Ok(io::sink()) }
}

impl HttpStream for io::Sink {
    type Request = ();
    type Response = ();
    type Error = io::Error;

    fn finish(self) -> Result<Self::Response, Self::Error> { Ok(()) }
}

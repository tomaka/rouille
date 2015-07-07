//! The client side implementation of `multipart/form-data` requests.
//!
//! Use this when sending POST requests with files to a server.
//!
//! `ChunkedMultipart` sends chunked requests (recommended), 
//! while `SizedMultipart` sends sized requests.
//!
//! Both implement the `MultipartRequest` trait for their core API.
//!
//! Sized requests are more human-readable and use less bandwidth 
//! (as chunking adds [significant visual noise and overhead][chunked-example]),
//! but they must be able to load their entirety, including the contents of all files
//! and streams, into memory so the request body can be measured and its size set
//! in the `Content-Length` header.
//!
//! You should really only use sized requests if you intend to inspect the data manually on the
//! server side, as it will produce a more human-readable request body. Also, of course, if the
//! server doesn't support chunked requests or otherwise rejects them. 
//!
//! [chunked-example]: http://en.wikipedia.org/wiki/Chunked_transfer_encoding#Example 
use mime::{Mime, TopLevel, SubLevel, Attr, Value};

use std::borrow::{Borrow, BorrowMut};
use std::cell::Cell;

use std::fs::File;
use std::io;
use std::io::prelude::*;

use std::marker::PhantomData;

pub type Multipart<R: HttpRequest> = ChunkedMultipart<R>;

/// The core API for all multipart requests.
///
/// As part of the API contract, all errors in writing to the HTTP stream are stashed until the
/// implementor's concrete `.send()` method is called, or if the last error is inspected with
/// `.last_err()`.
///
/// However, no reading or writing is performed after an error occurs, to avoid wasting CPU cycles
/// on a request that will not complete, with the exception of API calls after the last error is
/// cleared with `.take_err()`.
pub trait MultipartRequest {
    #[doc(hidden)]
    type Stream: Write;

    type Error: From<io::Error>;

    #[doc(hidden)]
    fn stream_mut(&mut self) -> &mut Self::Stream;
    #[doc(hidden)]
    fn write_boundary(&mut self) -> io::Result<()>;

    /// Get a reference to the last error returned from writing to the HTTP stream, if any.
    fn last_err(&self) -> Option<&Self::Error>;

    /// Remove and return the last error to occur, allowing subsequent API calls to proceed
    /// normally.
    fn take_err(&mut self) -> Option<Self::Error>;

    /// Write a text field to this multipart request.
    /// `name` and `val` can be either owned `String` or `&str`.
    fn write_text<N: Borrow<str>, V: Borrow<str>>(mut self, name: N, val: V) -> Self {
        if self.last_err.is_none() {
            self.last_err = chain_result! {
                self.write_field_headers(name, None, None),
                self.write_line(val.borrow()),
                self.write_boundary()
            }.err().map(Self::Error::from)
        }

        self
    }
    
    /// Write a file to the multipart request, guessing its `Content-Type`
    /// from its extension and supplying its filename.
    ///
    /// See `write_stream()` for more info.
    fn write_file<N: Borrow<str>, F: BorrowMut<File>>(&mut self, name: N, file: F) -> Self {
        if self.last_err.is_none() {     
            self.last_err = chain_result! {
                { // New borrow scope so we can reborrow `file` after
                    let file_path = file.borrow().path();
                    let content_type = ::mime_guess::guess_mime_type(file_path);
                    self.write_field_headers(name.borrow(), file_path.filename_str(), content_type)
                },
                io::copy(file.borrow_mut(), self.stream_mut()),
                self.write_boundary()
            }.err().map(Self::Error::from);
        }

        self
    }

    /// Write a byte stream to the multipart request as a file field, supplying `filename` if given,
    /// and `content_type` if given or `application/octet-stream` if not.
    ///
    /// ##Warning
    /// The given `Read` **must** be able to read to EOF (end of file/no more data), meaning
    /// `Read::read()` returns `Ok(0)`. 
    /// If it never returns EOF it will be read to infinity and the request will never be completed.
    ///
    /// In the case of `SizedMultipart` this also can cause out-of-control memory usage as the
    /// multipart data has to be written to an in-memory buffer so it can be measured.
    ///
    /// Use `Read::take` if you wish to send data from a `Read` that will never end otherwise.
    fn write_stream<N: Borrow<str>, F: Borrow<str>, Rt: Read, R: Borrow<Rt>>(
        mut self, name: N, read: R, filename: Option<F>, content_type: Option<Mime>
    ) -> Self {
        if self.last_err.is_none() {
            let content_type = content_type.map_or_else(::mime_guess::octet_stream);

            self.last_err = chain_result! {
                self.write_field_headers(name.borrow(), filename, content_type),
                io::copy(read.borrow_mut(), self.stream_mut()),
                self.write_boundary()
            }.err().map(Self::Error::from);
        }

        self
    } 

    #[doc(hidden)]
    fn write_field_headers(&mut self, name: &str, filename: Option<&str>, content_type: Option<Mime>) 
    -> io::Result<()> {
        chain_result! {
            write!(self.stream_mut(), "Content-Disposition: form-data; name=\"{}\"", name),
            filename.map(|filename| write!(self.stream_mut(), "; filename=\"{}\"", filename))
                .unwrap_or(Ok(())),
            content_type.map(|content_type| write!(self.stream_mut(), "\r\nContent-Type: {}", content_type))
                .unwrap_or(Ok(())),
            self.write_line("\r\n")
        }
    }

    #[doc(hidden)]
    fn write_line(&mut self, line: &str) -> io::Result<()> {
        write!(self.stream_mut(), "{}\r\n", line)
    }
}

/// The entry point of the client-side multipart API.
///
/// Add text fields with `.add_text()` and files with `.add_file()`,
/// then obtain a `hyper::client::Request` object and pass it to `.send()`.
pub struct ChunkedMultipart<R: HttpRequest> {
    request: PhantomData<R>,
    stream: R::Stream,
    boundary: String,
    last_err: Option<R::Error>,
}

impl<R: HttpRequest> ChunkedMultipart<R> {
    /// Create a new `ChunkedMultipart` to wrap a request.
    ///
    /// ##May Panic
    /// If `req` fails sanity checks in `HttpRequest::apply_headers()`.
    pub fn from_request(mut req: R) -> Result<Multipart<R>, R::Error> {
        let boundary = ::gen_boundary();
        req.apply_headers(&boundary, None);

        let stream = try!(req.open_stream());

        Multipart {
            request: PhantomData,
            stream: stream,
            boundary: boundary,
            last_err: None
        }
    }

    /// Finalize the request and return the response from the server.   
    pub fn send(self) -> <<R as HttpRequest>::Stream as HttpStream>::Result where R: HttpRequest {
        self.last_err.and_then(|_| self.stream.finalize())
    }    
}

impl<R: HttpRequest> MultipartRequest for ChunkedMultipart<R> {
    type Stream = R::Stream;
    type Error = R::Error;

    fn stream_mut(&mut self) -> &mut R::Stream { 
        &mut self.stream    
    }

    fn write_boundary(&mut self) -> io::Result<()> {
        write!(self.stream, "{}\r\n", self.boundary)
    } 

    fn last_err(&self) -> Option<&Self::Error> {
        self.last_err.as_ref()
    }

    fn take_err(&mut self) -> Option<Self::Error> {
        self.last_err.take()
    }
}

/// A struct for sending a sized multipart request. The API varies subtly from `Multipart`.
///
/// The request data will be written to a `Vec<u8>` so its size can be measured and the
/// `Content-Length` header set when the request is sent. 
pub struct SizedMultipart {
    data: Vec<u8>,
    boundary: String,    
    last_err: Option<io::Error>,
}

impl SizedMultipart {
    pub fn new() -> SizedMultipart {
        SizedMultipart {
            data: Vec::new(),
            boundary: ::gen_boundary(),
            last_err: None,
        }
    }

    pub fn send<R>(self, mut req: R) -> <<R as HttpRequest>::Stream as HttpStream>::Result 
    where R: HttpRequest {
        let boundary = ::gen_boundary();
        req.apply_headers(&boundary, Some(self.data.len()));
        let req = try!(req.open_stream());
        try!(io::copy(&self.data, &mut req));
        req.finish()
    }
}

impl MultipartRequest for SizedMultipart {
    type Stream = Vec<u8>;
    type Error = io::Error;

    #[doc(hidden)]
    fn stream_mut(&mut self) -> &mut Vec<u8> {
        &mut self.data 
    }

    #[doc(hidden)]
    fn write_boundary(&mut self) -> io::Result<()> {
        write!(self.stream, "{}\r\n", self.boundary)
    }

    fn last_err(&self) -> Option<&Self::Error> {
        self.last_err.as_ref()
    }

    fn take_err(&mut self) -> Option<Self::Error> {
        self.last_err.take()
    }
}

fn multipart_mime(bound: &str) -> Mime {
    Mime(
        TopLevel::Multipart, SubLevel::Ext("form-data".into_string()),
        vec![(Attr::Ext("boundary".into_string()), Value::Ext(bound.into_string()))]
    )         
}

pub trait HttpRequest {
    type Stream: HttpStream; 
    type Response: Read;
    type Error: From<io::Error>;

    /// Set the `ContentType` header to `multipart/form-data` and supply the `boundary` value.
    ///
    /// If `content_len` is given, set the `ContentLength` header to its value.
    fn apply_headers(&mut self, boundary: &str, content_len: Option<usize>);
    /// Open the request stream and invoke the given closure, where the request body will be
    /// written. After the closure returns, finalize the request and return its result.
    fn open_stream(self) -> Result<Self::Stream, Self::Error>;
}

pub trait HttpStream: Write {
    type Request: HttpRequest;
    type Result = Result<Self::Request::Response, Self::Request::Error>;

    /// Finalize and close the stream, returning the HTTP response object.
    fn finish(self) -> Self::Result;
}

#[cfg(feature = "hyper")]
mod hyper_impl {
    use hyper::client::request::Request;
    use hyper::client::response::Response;
    use hyper::error::Error as HyperError;
    use hyper::net::{Fresh, Streaming};

    use std::io;

    impl super::HttpRequest for Request<Fresh> {
        type Stream = Request<Streaming>;
        type Response = Response;
        type Error = HyperError;

        /// #Panics
        /// If the `Request<Fresh>` method is not `Method::Post`.
        fn apply_headers(&mut self, boundary: &str, content_len: Option<usize>) {
            use hyper::header::{ContentType, ContentLength};
            use hyper::method::Method;

            assert!(self.method() == Method::Post, "Multipart request must use POST method!");        

            let headers = self.headers_mut();

            headers.set(ContentType(super::multipart_mime(boundary)));

            if let Some(size) = content_len {
                headers.set(ContentLength(size));   
            }

            debug!("Hyper headers: {}", self.headers());        
        }

        fn send<F>(self, send_fn: F) -> Self::RequestResult 
            where F: FnOnce(&mut Request<Streaming>) -> io::Result<()> 
        {
            let mut req = try!(self.start());
            try!(send_fn(&mut req));
            req.send()
        }
    } 

    impl super::HttpStream for Request<Streaming> {
        
    }
}


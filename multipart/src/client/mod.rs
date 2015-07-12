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
use mime::Mime;

use std::borrow::{Borrow, BorrowMut};
use std::convert::AsRef;

use std::fs::File;
use std::io;
use std::io::prelude::*;

use std::path::Path;

#[cfg(feature = "hyper")]
mod hyper;

mod sized;

pub use self::sized::SizedRequest;


/// The entry point of the client-side multipart API.
///
/// Though they perform I/O, the `.write_*()` methods do not return `io::Result<_>` in order to
/// facilitate method chaining. Upon the first error, all subsequent API calls will be no-ops until
/// `.send()` is called, at which point the error will be reported.
///
/// If you don't want to consume data handles (`File`, etc.), `write_file()` and `write_stream()`
/// also accept `&mut` versions.
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
        let boundary = ::gen_boundary();
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
    pub fn take_err(&mut self) -> Option<S::Error> {
        self.last_err.take()
    }

    /// Write a text field to this multipart request.
    /// `name` and `val` can be either owned `String` or `&str`.
    ///
    /// ##Errors
    /// If something went wrong with the HTTP stream.
    pub fn write_text<N: Borrow<str>, V: Borrow<str>>(mut self, name: N, val: V) -> Self {
        if self.last_err.is_none() {
            self.last_err = chain_result! {
                self.write_field_headers(name.borrow(), None, None),
                self.write_line(val.borrow()),
                self.write_boundary()
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
    /// ##Errors
    /// If there was a problem opening the file (was a directory or didn't exist),
    /// or if something went wrong with the HTTP stream.
    // Remove `File::path()`, GG Rust-lang
    pub fn write_file<N: Borrow<str>, P: AsRef<Path>>(mut self, name: N, path: P) -> Self {
        if self.last_err.is_none() {     
            let path = path.as_ref();

            self.last_err = chain_result! {
                { // New borrow scope so we can reborrow `file` after
                    let content_type = ::mime_guess::guess_mime_type(path);
                    let filename = path.file_name().and_then(|filename| filename.to_str());
                    self.write_field_headers(name.borrow(), filename, Some(content_type))
                },
                File::open(path).and_then(|ref mut file| io::copy(file, &mut self.stream)),
                self.write_boundary()
            }.err().map(|err| err.into());
        }

        self
    }

    /// Write a byte stream to the multipart request as a file field, supplying `filename` if given,
    /// and `content_type` if given or `application/octet-stream` if not.
    ///
    /// ##Warning
    /// The given `Read` **must** be able to read to EOF (end of file/no more data), meaning
    /// `Read::read()` returns `Ok(0)`. If it never returns EOF it will be read to infinity 
    /// and the request will never be completed.
    ///
    /// When using `sized::SizedRequest` this also can cause out-of-control memory usage as the
    /// multipart data has to be written to an in-memory buffer so its size can be calculated.
    ///
    /// Use `Read::take` if you wish to send data from a `Read` that will never end otherwise.
    ///
    /// ##Errors
    /// If the reader returned an error, or if something went wrong with the HTTP stream.
    // RFC: How to format this declaration?
    pub fn write_stream<N: Borrow<str>, St: Read, St_: BorrowMut<St>>(
        mut self, name: N, mut read: St_, filename: Option<&str>, content_type: Option<Mime>
    ) -> Self {
        if self.last_err.is_none() {
            let content_type = content_type.unwrap_or_else(::mime_guess::octet_stream);

            self.last_err = chain_result! {
                self.write_field_headers(name.borrow(), filename, Some(content_type)),
                io::copy(read.borrow_mut(), &mut self.stream),
                self.write_boundary()
            }.err().map(|err| err.into());
        }

        self
    } 

    fn write_field_headers(&mut self, name: &str, filename: Option<&str>, content_type: Option<Mime>) 
    -> io::Result<()> {
        chain_result! {
            write!(self.stream, "Content-Disposition: form-data; name=\"{}\"", name),
            filename.map(|filename| write!(self.stream, "; filename=\"{}\"", filename))
                .unwrap_or(Ok(())),
            content_type.map(|content_type| write!(self.stream, "\r\nContent-Type: {}", content_type))
                .unwrap_or(Ok(())),
            self.write_line("\r\n")
        }
    }

    fn write_line(&mut self, line: &str) -> io::Result<()> {
        write!(self.stream, "{}\r\n", line)
    }

    fn write_boundary(&mut self) -> io::Result<()> {
        write!(self.stream, "{}\r\n", self.boundary)
            .map(|res| { self.data_written = true; res })
    }

    /// Finalize the request and return the response from the server, or the last error if set.
    pub fn send(mut self) -> Result<S::Response, S::Error> {
        match self.last_err {
            None => {
                if self.data_written {
                    try!(self.stream.write(b"--"));
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

pub trait HttpRequest {
    type Stream: HttpStream; 
    type Error: From<io::Error> + Into<<Self::Stream as HttpStream>::Error>;

    /// Set the `ContentType` header to `multipart/form-data` and supply the `boundary` value.
    /// If `content_len` is given, set the `Content-Length` header to its value.
    fn apply_headers(&mut self, boundary: &str, content_len: Option<usize>);

    /// Open the request stream and return it, after which point the request body will be
    /// written. `HttpStream::finish()` will be called after the body has finished being written.
    fn open_stream(self) -> Result<Self::Stream, Self::Error>;
}

pub trait HttpStream: Write {
    type Request: HttpRequest;
    type Response: Read;
    type Error: From<io::Error> + From<<Self::Request as HttpRequest>::Error>; 

    /// Finalize and close the stream, returning the HTTP response object.
    fn finish(self) -> Result<Self::Response, Self::Error>;
}

impl HttpRequest for () {
    type Stream = io::Sink;
    type Error = io::Error;

    fn apply_headers(&mut self, _: &str, _: Option<usize>) { }
    fn open_stream(self) -> Result<Self::Stream, Self::Error> { Ok(io::sink()) }
}

impl HttpStream for io::Sink {
    type Request = ();
    type Response = io::Empty;
    type Error = io::Error;

    fn finish(self) -> Result<Self::Response, Self::Error> { Ok(io::empty()) }
}

//! The client side implementation of `multipart/form-data` requests.
//!
//! Use this when sending POST requests with files to a server.
//!
//! See the `Multipart` struct for more info.
use mime::{Mime, TopLevel, SubLevel, Attr, Value};

use mime_guess::{guess_mime_type, octet_stream};

use std::borrow::Cow;

use std::fs::File;
use std::io;
use std::io::prelude::*;

use super::{MultipartField, MultipartFile, ref_copy, random_alphanumeric};

const BOUNDARY_LEN: usize = 8;

type Fields<'a> = Vec<(Cow<'a, str>, MultipartField<'a>)>;

/// The entry point of the client-side multipart API.
///
/// Add text fields with `.add_text()` and files with `.add_file()`,
/// then obtain a `hyper::client::Request` object and pass it to `.send()`.
pub struct Multipart<'a> {
    fields: Fields<'a>,
    boundary: String,
    /// If the request can be sized.
    /// If true, avoid using chunked requests.
    /// Defaults to `false`.
    pub sized: bool,
}

impl<'a> Multipart<'a> {

    /// Create a new `Multipart` instance with an empty set of fields.
    pub fn new() -> Multipart<'a> {
        Multipart {
            fields: Vec::new(),
            boundary: random_alphanumeric(BOUNDARY_LEN),
            sized: false,
        } 
    }

    /// Add a text field to this multipart request.
    /// `name` and `val` can be either owned `String` or `&str`.
    /// Prefer `String` if you're trying to limit allocations and copies.
    pub fn add_text<N: Into<Cow<'a, str>>, V: Into<Cow<'a, str>>>(&mut self, name: N, val: V) {
        self.add_field(name, MultipartField::Text(val));    
    }
    
    /// Add the file to the multipart request, guessing its `Content-Type`
    /// from its extension and supplying its filename.
    ///
    /// See `add_stream()`.
    pub fn add_file<N: Into<Cow<'a, str>>>(&mut self, name: N, file: &'a mut File) {
        let filename = file.path().filename_str().map(|s| s.into_string());
        let content_type = guess_mime_type(file.path());

        self.add_field(name, 
            MultipartField::File(MultipartFile::from_file(filename, file, content_type))
        );
    }

    /// Add a `Read` as a file field, supplying `filename` if given,
    /// and `content_type` if given or `application/octet-stream` if not.
    ///
    /// ##Warning
    /// The given `Read` **must** be able to read to EOF (end of file/no more data). 
    /// If it never returns EOF it will be read to infinity (even if it reads 0 bytes forever) 
    /// and the request will never be completed.
    ///
    /// If `sized` is `true`, this adds an additional consequence of out-of-control
    /// memory usage, as `Multipart` tries to read an infinite amount of data into memory.
    ///
    /// Use `std::io::util::LimitReader` if you wish to send data from a `Read`
    /// that will never return EOF otherwise.
    pub fn add_stream<N: Into<Cow<'a, str>>>(&mut self, name: N, reader: &'a mut (Read + 'a), 
        filename: Option<String>, content_type: Option<Mime>) {
        self.add_field(name,
            MultipartField::File(MultipartFile {
                filename: filename,
                content_type: content_type.unwrap_or_else(octet_stream),
                reader: reader,
                tmp_dir: None,
            })
        );        
    }

    fn add_field<N: Into<Cow<'a, str>>>(&mut self, name: N, val: MultipartField<'a>) {
        self.fields.push((name.into_string(), val));  
    }

    /// Apply the appropriate headers to the `Request<Fresh>` (obtained from Hyper) and send the data.
    /// If `self.sized == true`, send a sized (non-chunked) request, setting the `Content-Length`
    /// header. Else, send a chunked request.
    ///
    /// Sized requests are more human-readable and use less bandwidth 
    /// (as chunking adds [significant visual noise and overhead][chunked-example]),
    /// but they must be able to load their entirety, including the contents of all files
    /// and streams, into memory so the request body can be measured and its size set
    /// in the `Content-Length` header.
    ///
    /// Prefer chunked requests when sending very large or numerous files,
    /// or when human-readability or bandwidth aren't an issue.
    ///
    /// [chunked-example]: http://en.wikipedia.org/wiki/Chunked_transfer_encoding#Example 
    ///
    /// ##Panics
    /// If `req` fails sanity checks in `HttpRequest::apply_headers()`.
    pub fn send<R>(self, mut req: R) -> R::RequestResult where R: HttpRequest {
        debug!("Fields: {}; Boundary: {}", self.fields, self.boundary);

        if self.sized {
            return self.send_sized(req);    
        }

        let Multipart { fields, boundary, ..} = self;

        req.apply_headers(&boundary, None);

        req.send(|req| write_body(&mut req, fields, &boundary))
    }
 
    fn send_sized<R>(self, mut req: R) -> R::RequestResult where R: HttpRequest {
        let mut body: Vec<u8> = Vec::new();

        let Multipart { fields, boundary, ..} = self;

        try!(write_body(&mut body, fields, boundary));
        
        req.apply_headers(&boundary, Some(body.len()));
        req.send(|req| req.write(&body))
    }    
}


fn write_body<W: io::Write>(wrt: &mut W, fields: Fields, boundary: &str) -> io::Result<()> {
    try!(write_boundary(wrt, boundary));

    for (name, field) in fields.into_iter() {
        try!(write_field(wrt, name, field, boundary));
    }

    Ok(())
} 

fn write_field(wrt: &mut Write, name: String, field: MultipartField, boundary: &str) -> io::Result<()> {
    try!(write!(wrt, "Content-Disposition: form-data; name=\"{}\"\r\n\r\n", name));

    try!(match field {
            MultipartField::Text(text) => write_line(wrt, &*text),
            MultipartField::File(file) => write_file(wrt, file),
        });
    
    write_boundary(wrt, boundary)  
} 

fn write_boundary(wrt: &mut Write, boundary: &str) -> io::Result<()> {
    write!(wrt, "--{}\r\n", boundary)
}

fn write_file(wrt: &mut Write, mut file: MultipartFile) -> io::Result<()> {
    try!(file.filename.map(|filename| write!(wrt, "; filename=\"{}\"\r\n", filename)).unwrap_or(Ok(())));
    try!(write!(wrt, "Content-Type: {}\r\n\r\n", file.content_type));
    ref_copy(&mut file.reader, wrt)         
}

/// Specialized write_line that writes CRLF after a line as per W3C specs
fn write_line(req: &mut Write, s: &str) -> io::Result<()> {
    req.write_str(s).and_then(|_| req.write(b"\r\n"))        
}

fn multipart_mime(bound: &str) -> Mime {
    Mime(
        TopLevel::Multipart, SubLevel::Ext("form-data".into_string()),
        vec![(Attr::Ext("boundary".into_string()), Value::Ext(bound.into_string()))]
    )         
}

pub trait HttpRequest {
    type RequestStream: Write;
    type Response;
    type RequestErr: From<io::Error>;
    type RequestResult = Result<Self::Response, Self::RequestErr>;

    /// Set the `ContentType` header to `multipart/form-data` and supply the `boundary` value.
    ///
    /// If `content_len` is given, set the `ContentLength` header to its value.
    fn apply_headers(&mut self, boundary: &str, content_len: Option<usize>);
    /// Open the request stream and invoke the given closure, where the request body will be
    /// written. After the closure returns, finalize the request and return its result.
    fn send<F>(self, send_fn: F) -> Self::RequestResult 
        where F: FnOnce(&mut Self::RequestStream) -> io::Result<()>;
}

#[cfg(feature = "hyper")]
mod hyper_impl {
    use hyper::client::request::Request;
    use hyper::client::response::Response;
    use hyper::error::Error as HyperError;
    use hyper::net::{Fresh, Streaming};

    use std::io;

    impl super::HttpRequest for Request<Fresh> {
        type RequestStream = Request<Streaming>;
        type Response = Response;
        type RequestErr = HyperError;

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
}


//! The client side implementation of `multipart/form-data` requests.
//!
//! Use this when sending POST requests with files to a server.
//!
//! See the `Multipart` struct for more info.

use hyper::client::{Request, Response};

use hyper::header::common::{ContentType, ContentLength};

use hyper::net::{Fresh, Streaming};
use hyper::{HttpResult, HttpIoError};

use mime::{Mime, TopLevel, SubLevel, Attr, Value};

use mime_guess::{guess_mime_type, octet_stream};

use std::io::IoResult;
use std::io::fs::File;

use super::{MultipartField, MultipartFile, ref_copy, random_alphanumeric};

const BOUNDARY_LEN: uint = 8;

type Fields<'a> = Vec<(String, MultipartField<'a>)>;


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

/// Shorthand for a writable request (`Request<Streaming>`)
type ReqWrite = Request<Streaming>;

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
    pub fn add_text<N: StrAllocating, V: StrAllocating>(&mut self, name: N, val: V) {
        self.add_field(name, MultipartField::Text(val.into_string()));    
    }
    
    /// Add the file to the multipart request, guessing its `Content-Type`
    /// from its extension and supplying its filename.
    ///
    /// See `add_stream()`.
    pub fn add_file<N: StrAllocating>(&mut self, name: N, file: &'a mut File) {
        let filename = file.path().filename_str().map(|s| s.into_string());
        let content_type = guess_mime_type(file.path());

        self.add_field(name, 
            MultipartField::File(MultipartFile::from_file(filename, file, content_type))
        );
    }

    /// Add a `Reader` as a file field, supplying `filename` if given,
    /// and `content_type` if given or `application/octet-stream` if not.
    ///
    /// ##Warning
    /// The given `Reader` **must** be able to read to EOF (end of file/no more data). 
    /// If it never returns EOF it will be read to infinity (even if it reads 0 bytes forever) 
    /// and the request will never be completed.
    ///
    /// If `sized` is `true`, this adds an additional consequence of out-of-control
    /// memory usage, as `Multipart` tries to read an infinite amount of data into memory.
    ///
    /// Use `std::io::util::LimitReader` if you wish to send data from a `Reader`
    /// that will never return EOF otherwise.
    pub fn add_stream<N: StrAllocating>(&mut self, name: N, reader: &'a mut Reader + 'a, 
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

    fn add_field<N: StrAllocating>(&mut self, name: N, val: MultipartField<'a>) {
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
    /// If `req` is not a POST request, created with `Request::post()` or passing
    /// `hyper::method::Post` to `Request::new()`.
    pub fn send(self, mut req: Request<Fresh>) -> HttpResult<Response> {
        use hyper::method;
        assert!(req.method() == method::Post, "Multipart request must use POST method!");

        debug!("Fields: {}; Boundary: {}", self.fields[], self.boundary[]);

        if self.sized {
            return self.send_sized(req);    
        }

        let Multipart { fields, boundary, ..} = self;

        apply_headers(&mut req, boundary[], None);

        debug!("{}", req.headers());
        
        let mut req = try!(req.start());
        try!(io_to_http(write_body(&mut req, fields, boundary[])));
        req.send()
    }
 
    fn send_sized(self, mut req: Request<Fresh>) -> HttpResult<Response> {
        let mut body: Vec<u8> = Vec::new();

        let Multipart { fields, boundary, ..} = self;

        try!(write_body(&mut body, fields, boundary[]));
        
        apply_headers(&mut req, boundary[], Some(body.len()));
        
        let mut req = try!(req.start());
        try!(io_to_http(req.write(body[])));
        req.send()
    }    
}

fn apply_headers(req: &mut Request<Fresh>, boundary: &str, size: Option<uint>){
    let headers = req.headers_mut();

    headers.set(ContentType(multipart_mime(boundary)));

    if let Some(size) = size {
        headers.set(ContentLength(size));   
    }
}   

fn write_body<'a>(wrt: &mut Writer, fields: Fields<'a>, boundary: &str) -> IoResult<()> {
    try!(write_boundary(wrt, boundary[]));

    for (name, field) in fields.into_iter() {
        try!(write_field(wrt, name, field, boundary));
    }

    Ok(())
} 

fn write_field(wrt: &mut Writer, name: String, field: MultipartField, boundary: &str) -> IoResult<()> {
    try!(write!(wrt, "Content-Disposition: form-data; name=\"{}\"\r\n\r\n", name));

    try!(match field {
            MultipartField::Text(text) => write_line(wrt, &*text),
            MultipartField::File(file) => write_file(wrt, file),
        });
    
    write_boundary(wrt, boundary[])  
} 

fn write_boundary(wrt: &mut Writer, boundary: &str) -> IoResult<()> {
    write!(wrt, "--{}\r\n", boundary)
}

fn write_file(wrt: &mut Writer, mut file: MultipartFile) -> IoResult<()> {
    try!(file.filename.map(|filename| write!(wrt, "; filename=\"{}\"\r\n", filename)).unwrap_or(Ok(())));
    try!(write!(wrt, "Content-Type: {}\r\n\r\n", file.content_type));
    ref_copy(&mut file.reader, wrt)         
}

/// Specialized write_line that writes CRLF after a line as per W3C specs
fn write_line(req: &mut Writer, s: &str) -> IoResult<()> {
    req.write_str(s).and_then(|_| req.write(b"\r\n"))        
}


fn io_to_http<T>(res: IoResult<T>) -> HttpResult<T> {
    res.map_err(|e| HttpIoError(e))
}

fn multipart_mime(bound: &str) -> Mime {
    Mime(
        TopLevel::Multipart, SubLevel::Ext("form-data".into_string()),
        vec![(Attr::Ext("boundary".into_string()), Value::Ext(bound.into_string()))]
    )         
}



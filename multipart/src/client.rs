use hyper::client::{Request, Response};
use hyper::header::common::ContentType;
use hyper::net::{Fresh, Streaming};
use hyper::{HttpResult, HttpIoError};

use mime::{mod, Mime};

use serialize::json;

use std::cell::RefCell;

use std::collections::HashMap;

use std::io::IoResult;
use std::io::fs::File;
use std::io;

use super::{MultipartField, TextField, FileField, FileStream, MultipartFile};

const BOUNDARY_LEN: uint = 8;

pub struct Multipart<'a> {
    fields: Vec<(&'a str, MultipartField<'a>)>,
    boundary: String,
}

/// Shorthand for a writable request (`Request<Streaming>`)
type ReqWrite = Request<Streaming>;

impl<'a> Multipart<'a> {

    pub fn new() -> Multipart<'a> {
        Multipart {
            fields: Vec::new(),
            boundary: random_alphanumeric(BOUNDARY_LEN),
        } 
    }

    pub fn add_text(&mut self, name: &'a str, val: &'a str) {
        self.fields.push((name, TextField(val)));    
    }
    
    /// Add the file to the multipart request, guessing its `Content-Type` from its extension
    pub fn add_file(&mut self, name: &'a str, file: &'a mut File) {
        let filename = file.path().filename_str().map(|s| s.into_string());
        let content_type = guess_mime_type(file.path());

        self.fields.push((name, FileField(MultipartFile {
            filename: filename,
            reader: FileStream(file),
            content_type: content_type,                  
        })))
    }

    /// Apply the appropriate headers to the `Request<Fresh>` and send the data.
    pub fn send(self, mut req: Request<Fresh>) -> HttpResult<Response> {
        self.apply_headers(&mut req);

        debug!("{}", req.headers());

        let mut req = try!(req.start());
        try!(io_to_http(self.write_request(&mut req)));
        req.send()
    }
    
    fn apply_headers(&self, req: &mut Request<Fresh>){
        let headers = req.headers_mut();

        headers.set(ContentType(multipart_mime(self.boundary[])))         
    }

    fn write_request(self, req: &mut ReqWrite) -> IoResult<()> {
        let Multipart{ fields, boundary } = self;

        try!(write_boundary(req, boundary[]));

        for (name, field) in fields.into_iter() {
            try!(write!(req, "Content-Disposition: form-data; name=\"{}\"", name));

            try!(match field {
                    TextField(text) => req.write(b"\r\n\r\n")
                        .and_then(|_| write_line(req, text)), // Style suggestions welcome
                    FileField(file) => write_file(req, file),
                    _ => unimplemented!(),
                });
            
            try!(write_boundary(req, boundary[]));     
        }

        Ok(())
    }

}

fn write_boundary(req: &mut ReqWrite, boundary: &str) -> IoResult<()> {
    write!(req, "--{}\r\n", boundary)
}

fn write_file<'a>(req: &mut ReqWrite, mut file: MultipartFile<'a>) -> IoResult<()> {
    try!(file.filename.map(|filename| write!(req, "; filename=\"{}\"\r\n", filename)).unwrap_or(Ok(())));
    try!(write!(req, "Content-Type: {}\r\n\r\n", file.content_type));
    io::util::copy(&mut file.reader, req)         
}

/// Specialized write_line that writes CRLF after a line as per W3C specs
fn write_line(req: &mut ReqWrite, s: &str) -> IoResult<()> {
    req.write_str(s).and_then(|_| req.write(b"\r\n"))        
}

/// Generate a random alphanumeric sequence of length `len`
fn random_alphanumeric(len: uint) -> String {
    use std::rand::{task_rng, Rng};
    
    task_rng().gen_ascii_chars().take(len).collect()    
}

fn io_to_http<T>(res: IoResult<T>) -> HttpResult<T> {
    res.map_err(|e| HttpIoError(e))
}

fn multipart_mime(bound: &str) -> Mime {
    mime::Mime(
        mime::Multipart, mime::SubExt("form-data".into_string()),
        vec![(mime::AttrExt("boundary".into_string()), mime::ValueExt(bound.into_string()))]
    )         
}


/// Guess the MIME type of the `Path` by its extension.
///
/// **Guess** is the operative word here, as the contents of a file
/// may not or may not match its MIME type/extension.
pub fn guess_mime_type(path: &Path) -> Mime {
    let ext = path.extension_str().unwrap_or("");
    
    get_mime_type(ext)
}

local_data_key!(mime_types_cache: RefCell<HashMap<String, Mime>>)

/// Get the MIME type associated with a file extension
// MIME Types are cached in a task-local heap
pub fn get_mime_type(ext: &str) -> Mime {
    if ext.is_empty() { return octet_stream(); }

    let ext = ext.into_string();
   
    let cache = if let Some(cache) = mime_types_cache.get() { cache }
    else {
        mime_types_cache.replace(Some(RefCell::new(HashMap::new())));
        mime_types_cache.get().unwrap()   
    };

    if let Some(mime_type) = cache.borrow().find(&ext) {
        return mime_type.clone();   
    }

    let mime_type = find_mime_type(&ext);

    cache.borrow_mut().insert(ext, mime_type.clone());

    mime_type  
}

const MIME_TYPES: &'static str = include_str!("../mime_types.json");

/// Load the MIME_TYPES as JSON and try to locate `ext`
fn find_mime_type(ext: &String) -> Mime {
    json::from_str(MIME_TYPES).unwrap()
        .find(ext).and_then(|j| j.as_string())
        .and_then(from_str::<Mime>)
        .unwrap_or_else(octet_stream)
}

fn octet_stream() -> Mime {
    Mime(mime::Application, mime::SubExt("octet-stream".into_string()), Vec::new())   
}

#[test]
fn test_mime_type_guessing() {
    assert!(get_mime_type("gif").to_string() == "image/gif".to_string());
    assert!(get_mime_type("txt").to_string() == "text/plain".to_string());
    assert!(get_mime_type("blahblah").to_string() == "application/octet-stream".to_string());     
}


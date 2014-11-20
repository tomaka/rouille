#![feature(if_let, slicing_syntax, default_type_params, phase, unboxed_closures, macro_rules)]
extern crate hyper;
#[phase(plugin, link)] extern crate log;

extern crate mime;
extern crate serialize;

use self::mime::Mime;

use std::fmt::{Formatter, FormatError, Show};
use std::kinds::marker;
use std::io::fs::File;
use std::io::{AsRefReader, RefReader, IoResult};

use server::BoundaryReader;

pub mod client;
pub mod server;


pub struct MultipartFile<'a> {
    filename: Option<String>,
    content_type: Mime,
    reader: &'a mut Reader + 'a,
}

impl<'a> MultipartFile<'a> {
    fn from_octet(filename: Option<String>, reader: &'a mut Reader, cont_type: &str) -> MultipartFile<'a> {
        MultipartFile {
            filename: filename,
            reader: reader,
            content_type: from_str(cont_type).unwrap_or_else(mime_guess::octet_stream),
        }    
    }

    fn from_file(filename: Option<String>, reader: &'a mut File, mime: Mime) -> MultipartFile<'a> {
        MultipartFile {
            filename: filename,
            reader: reader,
            content_type: mime,
        }
    }
}

impl<'a> Show for MultipartFile<'a> {
    fn fmt(&self, fmt: &mut Formatter) -> Result<(), FormatError> {
        write!(fmt, "Filename: {} Content-Type: {}", self.filename, self.content_type)    
    } 
}

#[deriving(Show)]
pub enum MultipartField<'a> {
    Text(String),
    File(MultipartFile<'a>),
    // MultiFiles(Vec<MultipartFile>), /* TODO: Multiple files */
}

impl<'a> Reader for MultipartFile<'a> {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<uint>{
        self.reader.read(buf)
    }
}

pub mod mime_guess {
    
    use mime::{Mime, TopLevel, SubLevel};

    use serialize::json;
    
    use std::cell::RefCell;

    use std::collections::HashMap;


    /// Guess the MIME type of the `Path` by its extension.
    ///
    /// **Guess** is the operative word here, as the contents of a file
    /// may not or may not match its MIME type/extension.
    pub fn guess_mime_type(path: &Path) -> Mime {
        let ext = path.extension_str().unwrap_or("");
        
        get_mime_type(ext)
    }

    pub fn guess_mime_type_filename(filename: &str) -> Mime {
        let path = Path::new(filename);
        
        guess_mime_type(&path)    
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

        let mime_type = find_mime_type(&*ext);

        cache.borrow_mut().insert(ext, mime_type.clone());

        mime_type  
    }

    const MIME_TYPES: &'static str = include_str!("../mime_types.json");

    /// Load the MIME_TYPES as JSON and try to locate `ext`
    fn find_mime_type(ext: &str) -> Mime {
        json::from_str(MIME_TYPES).unwrap()
            .find(ext).and_then(|j| j.as_string())
            .and_then(from_str::<Mime>)
            .unwrap_or_else(octet_stream)
    }

    pub fn octet_stream() -> Mime {
        Mime(TopLevel::Application, SubLevel::Ext("octet-stream".into_string()), Vec::new())   
    }

#[test]
    fn test_mime_type_guessing() {
        assert!(get_mime_type("gif").to_string() == "image/gif".to_string());
        assert!(get_mime_type("txt").to_string() == "text/plain".to_string());
        assert!(get_mime_type("blahblah").to_string() == "application/octet-stream".to_string());     
    }
   
}

#[cfg(test)]
mod test {
   use hyper::Url;
   use hyper::client::request::Request as ClientReq;
   use client::Multipart as ClientMulti;

    #[test]
    fn client_api_test() {        
        let request = ClientReq::post(Url::parse("http://localhost:1337/").unwrap()).unwrap();

        let mut multipart = ClientMulti::new();

        multipart.add_text("hello", "world");
        multipart.add_text("goodnight", "sun");

        multipart.send(request).unwrap();        
    }
       
}

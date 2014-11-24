#![feature(if_let, slicing_syntax, default_type_params, phase, unboxed_closures, macro_rules)]
extern crate hyper;
#[phase(plugin, link)] extern crate log;

extern crate mime;
extern crate serialize;

use self::mime::Mime;

use std::fmt::{Formatter, Show};
use std::fmt::Error as FormatError;

use std::io::{File, IoErrorKind, IoResult};

use std::io::fs::PathExtensions;

pub mod client;
pub mod server;
pub mod mime_guess;

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

    /// Save this file to `path`, ignoring the filename, if any.
    ///
    /// Returns the created file on success.
    pub fn save_as(&mut self, path: &Path) -> IoResult<File> {
        let mut file = try!(File::create(path));

        try!(ref_copy(self.reader, &mut file));

        Ok(file)
    }

    /// Save this file in the directory described by `dir`,
    /// appending `filename` if any, or a random string.
    ///
    /// Returns the created file on success.
    ///
    /// ###Panics
    /// If `dir` does not represent a directory.
    pub fn save_in(&mut self, dir: &Path) -> IoResult<File> {
        assert!(dir.is_dir(), "Given path is not a directory!");

        let filename = self.filename.as_ref().map_or_else(|| random_alphanumeric(10), |s| s.clone());
        let path = dir.join(filename);
       
        self.save_as(&path)
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

/// A copy of `std::io::util::copy` that takes trait references
pub fn ref_copy(r: &mut Reader, w: &mut Writer) -> IoResult<()> {
    let mut buf = [0, ..1024 * 64];
    
    loop {
        let len = match r.read(&mut buf) {
            Ok(len) => len,
            Err(ref e) if e.kind == IoErrorKind::EndOfFile => return Ok(()),
            Err(e) => return Err(e),
        };
        try!(w.write(buf[..len]));
    }
}

/// Generate a random alphanumeric sequence of length `len`
fn random_alphanumeric(len: uint) -> String {
    use std::rand::{task_rng, Rng};

    task_rng().gen_ascii_chars().map(|ch| ch.to_lowercase()).take(len).collect()    
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
        //multipart.sized = true;

        multipart.send(request).unwrap();        
    }
       
}

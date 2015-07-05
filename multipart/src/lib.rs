#[macro_use] extern crate log;

extern crate mime;
extern crate mime_guess;
extern crate rand;
extern crate rustc_serialize;

#[cfg(feature = "hyper")]
extern crate hyper;

use mime::Mime;

use std::borrow::Cow;
use std::fmt;

use std::fs::File;
use std::io;
use std::io::prelude::*;
use std::path::{Path, PathBuf};

pub mod client;
pub mod server;

/// A representation of a file in HTTP `multipart/form-data`.
///
/// This struct has an input "flavor" and an output "flavor".
/// The input "flavor" is used internally by `client::Multipart::add_file()`
/// and is never exposed to the user.
///
/// The output "flavor" is returned by `server::Multipart::read_entry()` and represents
/// a file entry in the incoming multipart request. 
///
/// Note that in the output "flavor", the file is not yet saved to the system; 
/// instead, the struct implements a `Reader` that points 
/// to the beginning of the file's contents in the HTTP stream. 
/// You can read it to EOF, or use one of the `save_*()` methods here 
/// to save it to disk.
pub struct MultipartFile<'a> {
    filename: Option<String>,
    content_type: Mime,
    reader: &'a mut (Read + 'a),
}

impl<'a> MultipartFile<'a> {
    fn from_octet(filename: Option<String>, reader: &'a mut Read, cont_type: &str) -> MultipartFile<'a> {
        MultipartFile {
            filename: filename,
            reader: reader,
            content_type: cont_type.parse::<Mime>().ok().unwrap_or_else(mime_guess::octet_stream),
        }    
    }

    fn from_file(filename: Option<String>, reader: &'a mut File, mime: Mime) -> MultipartFile<'a> {
        MultipartFile {
            filename: filename,
            reader: reader,
            content_type: mime,
        }
    }

    /// Save this file to `path`, discarding the filename.
    ///
    /// If successful, the file can be found at `path`.
    pub fn save_as(&mut self, path: &Path) -> io::Result<()> {
        let mut file = try!(File::create(path));

        ref_copy(self.reader, &mut file)
    }

    /// Save this file in the directory described by `dir`,
    /// appending `filename` if present, or a random string otherwise.
    ///
    /// Returns the created file's path on success.
    ///
    /// ###Panics
    /// If `dir` does not represent a directory.
    pub fn save_in(&mut self, dir: &Path) -> io::Result<PathBuf> {
        assert!(dir.is_dir(), "Given path is not a directory!");

        let path = dir.join(self.dest_filename());
       
        try!(self.save_as(&path));

        Ok(path)
    }

    /// Save this file in the OS temp directory, returned from `std::env::temp_dir()`.
    ///
    /// Returns the created file's path on success.
    pub fn save_temp(&mut self) -> io::Result<PathBuf> {
        use std::env;
        
        self.save_in(&env::temp_dir())
    }

    fn dest_filename(&self) -> String {
        self.filename.as_ref().map_or_else(|| random_alphanumeric(10), |s| s.clone())
    }

    pub fn filename(&self) -> Option<&str> {
        self.filename.as_ref().map(String::as_ref)    
    }

    /// Get the content type of this file.
    /// On the client, it is guessed by the file extension.
    /// On the server, it is retrieved from the request or assumed to be
    /// `application/octet-stream`.
    pub fn content_type(&self) -> Mime {
        self.content_type.clone()    
    }
}

impl<'a> fmt::Debug for MultipartFile<'a> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "Filename: {:?} Content-Type: {:?}", self.filename, self.content_type)    
    } 
}


/// A field in a `multipart/form-data` request.
///
/// Like `MultipartFile`, this is used in both the client and server-side implementations,
/// but only exposed to the user on the server.
///
/// This enum does not include the names of the fields, as those are yielded separately
/// by `server::Multipart::read_entry()`.
#[derive(Debug)]
pub enum MultipartField<'a> {
    /// A text field.
    Text(Cow<'a, str>),
    /// A file field, including the content type and optional filename
    /// along with a `Read` implementation for getting the contents.
    File(MultipartFile<'a>),
    // MultiFiles(Vec<MultipartFile>), /* TODO: Multiple files */
}

impl<'a> MultipartField<'a> {

    /// Borrow this field as a text field, if possible.
    pub fn as_text(&self) -> Option<&str> {
        match *self {
            MultipartField::Text(ref s) => Some(s),
            _ => None,
        }
    }

    /// Take this field as a text field, if possible,
    /// returning `self` otherwise.
    pub fn to_text(self) -> Result<Cow<'a, str>, MultipartField<'a>> {
        match self {
            MultipartField::Text(s) => Ok(s),
            _ => Err(self),
        }
    }

    /// Borrow this field as a file field, if possible
    /// Mutably borrows so the contents can be read.
    pub fn as_file(&mut self) -> Option<&mut MultipartFile<'a>> {
        match *self {
            MultipartField::File(ref mut file) => Some(file),
            _ => None,
        }
    }

    /// Take this field as a file field if possible,
    /// returning `self` otherwise.
    pub fn to_file(self) -> Result<MultipartFile<'a>, MultipartField<'a>> {
        match self {
            MultipartField::File(file) => Ok(file),
            _ => Err(self),
        }
    }
}

impl<'a> Read for MultipartFile<'a> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>{
        self.reader.read(buf)
    }
}

/// A copy of `std::io::util::copy` that takes trait references
pub fn ref_copy(r: &mut Read, w: &mut Write) -> io::Result<()> {
    let mut buf = [0; 1024 * 64];
    
    loop {
        let len = try!(r.read(&mut buf)); 
        
        if len == 0 { break; }

        try!(w.write(&buf[..len]));
    }

    Ok(())
}

/// Generate a random alphanumeric sequence of length `len`
fn random_alphanumeric(len: usize) -> String {
    use rand::Rng;

    rand::thread_rng().gen_ascii_chars().flat_map(|ch| ch.to_lowercase()).take(len).collect()    
}


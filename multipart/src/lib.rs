#![feature(if_let, slicing_syntax, default_type_params, phase, unboxed_closures, macro_rules)]
extern crate hyper;
#[phase(plugin, link)] extern crate log;

extern crate mime;
extern crate serialize;

use self::mime::Mime;

use std::fmt::{Formatter, Show};
use std::fmt::Error as FormatError;

use std::io::{File, IoErrorKind, IoResult, TempDir};

use std::io::fs::PathExtensions;

pub mod client;
pub mod server;
pub mod mime_guess;

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
    reader: &'a mut (Reader + 'a),
    tmp_dir: Option<&'a str>,
}

impl<'a> MultipartFile<'a> {
    fn from_octet(filename: Option<String>, reader: &'a mut Reader, cont_type: &str, tmp_dir: &'a str) -> MultipartFile<'a> {
        MultipartFile {
            filename: filename,
            reader: reader,
            content_type: from_str(cont_type).unwrap_or_else(mime_guess::octet_stream),
            tmp_dir: Some(tmp_dir),
        }    
    }

    fn from_file(filename: Option<String>, reader: &'a mut File, mime: Mime) -> MultipartFile<'a> {
        MultipartFile {
            filename: filename,
            reader: reader,
            content_type: mime,
            tmp_dir: None,
        }
    }

    /// Save this file to `path`, discarding the filename.
    ///
    /// If successful, the file can be found at `path`.
    pub fn save_as(&mut self, path: &Path) -> IoResult<()> {
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
    pub fn save_in(&mut self, dir: &Path) -> IoResult<Path> {
        assert!(dir.is_dir(), "Given path is not a directory!");

        let path = dir.join(self.dest_filename());
       
        try!(self.save_as(&path));

        Ok(path)
    }

    /// Save this file in the temp directory `tmpdir` if supplied,
    /// or a random subdirectory under `std::os::tmp_dir()` otherwise. 
    /// The same directory is used for all files in the same request).
    ///
    ///
    /// Returns the created file's path on success.
    pub fn save_temp(&mut self, tmp_dir: Option<&TempDir>) -> IoResult<Path> {
        use std::os;

        let dir = match tmp_dir {
            Some(tmp_dir) => tmp_dir.path().clone(),
            None => os::tmpdir().join(self.tmp_dir.unwrap()),
        };
        
        self.save_in(&dir)
    }

    fn dest_filename(&self) -> String {
        self.filename.as_ref().map_or_else(|| random_alphanumeric(10), |s| s.clone())
    }

    pub fn filename(&self) -> Option<&str> {
        self.filename.as_ref().map(|s| s[])    
    }

    /// Get the content type of this file.
    /// On the client, it is guessed by the file extension.
    /// On the server, it is retrieved from the request or assumed to be
    /// `application/octet-stream`.
    pub fn content_type(&self) -> Mime {
        self.content_type.clone()    
    }
}

impl<'a> Show for MultipartFile<'a> {
    fn fmt(&self, fmt: &mut Formatter) -> Result<(), FormatError> {
        write!(fmt, "Filename: {} Content-Type: {}", self.filename, self.content_type)    
    } 
}


/// A field in a `multipart/form-data` request.
///
/// Like `MultipartFile`, this is used in both the client and server-side implementations,
/// but only exposed to the user on the server.
///
/// This enum does not include the names of the fields, as those are yielded separately
/// by `server::Multipart::read_entry()`.
#[deriving(Show)]
pub enum MultipartField<'a> {
    /// A text field.
    Text(String),
    /// A file field, including the content type and optional filename
    /// along with a `Reader` implementation for getting the contents.
    File(MultipartFile<'a>),
    // MultiFiles(Vec<MultipartFile>), /* TODO: Multiple files */
}

impl<'a> MultipartField<'a> {

    /// Borrow this field as a text field, if possible.
    pub fn as_text<'a>(&'a self) -> Option<&'a str> {
        match *self {
            MultipartField::Text(ref s) => Some(s[]),
            _ => None,
        }
    }

    /// Take this field as a text field, if possible,
    /// returning `self` otherwise.
    pub fn to_text(self) -> Result<String, MultipartField<'a>> {
        match self {
            MultipartField::Text(s) => Ok(s),
            _ => Err(self),
        }
    }

    /// Borrow this field as a file field, if possible
    /// Mutably borrows so the contents can be read.
    pub fn as_file<'b>(&'b mut self) -> Option<&'b mut MultipartFile<'a>> {
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


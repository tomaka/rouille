//! The server-side implementation of `multipart/form-data` requests.
//!
//! Use this when you are implementing a server on top of Hyper and want to
//! to parse and serve POST `multipart/form-data` requests.
//!
//! See the `Multipart` struct for more info.

use mime::{Mime, TopLevel, SubLevel, Attr, Value};

use std::borrow::Borrow;
use std::cmp;
use std::collections::HashMap;
use std::ops::Deref;

use std::fmt;

use std::fs::{self, File};
use std::io;
use std::io::prelude::*;

use std::path::{Path, PathBuf};

#[doc(inline)]
pub use self::boundary::BoundaryReader;

mod boundary;

#[cfg(feature = "hyper")]
pub mod hyper;

/// The server-side implementation of `multipart/form-data` requests.
///
/// Create this with `Multipart::from_request()` passing a `server::Request` object from Hyper,
/// or give Hyper a `handler::Switch` instance instead,
/// then read individual entries with `.read_entry()` or process them all at once with
/// `.foreach_entry()`.
///
/// Implements `Deref<Request>` to allow access to read-only fields on `Request` without copying.
pub struct Multipart<R> {
    source: BoundaryReader<R>,
    line_buf: String,
}

macro_rules! try_find(
    ($needle:expr, $haystack:expr, $err:expr) => (
        try!($haystack.find($needle).ok_or_else(|| line_error($err, $haystack)))
    )
);

impl<R> Multipart<R> where R: HttpRequest {
    /// If the given `HttpRequest` is a POST request of `Content-Type: multipart/form-data`,
    /// return the wrapped request as `Ok(Multipart)`, otherwise `Err(HttpRequest)`.
    pub fn from_request(req: R) -> Result<Multipart<R>, R> {
        if !req.is_multipart() { return Err(req); }

        if req.get_boundary().is_none() {
            return Err(req);     
        }

        let boundary = req.get_boundary().unwrap().into();

        debug!("Boundary: {}", boundary);

        Ok(
            Multipart { 
                source: BoundaryReader::from_reader(req, boundary),
                line_buf: String::new(),
            }
        )
    }

    /// Read an entry from this multipart request, returning a pair with the field's name and
    /// contents. This will return an End of File error if there are no more entries.
    ///
    /// To get to the data, you will need to match on `MultipartField`.
    ///
    /// ##Warning
    /// If the last returned entry had contents of type `MultipartField::File`,
    /// calling this again will discard any unread contents of that entry!
    pub fn read_entry(&mut self) -> io::Result<(String, MultipartField<R>)> {
        try!(self.source.consume_boundary());
        let (disp_type, field_name, filename) = try!(self.read_content_disposition());

        if disp_type != "form-data" {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("Content-Disposition value: {:?} expected: \"form-data\"", disp_type),
            ));
        }

        if let Some(content_type) = try!(self.read_content_type()) {
            let _ = try!(self.read_line()); // Consume empty line
            Ok((field_name,
                MultipartField::File(
                    MultipartFile::from_reader(filename, &mut self.source, &content_type)
                )
            ))
        } else {
            // Empty line consumed by read_content_type()
            let text = try!(self.read_to_string());
            // The last two characters are "\r\n".
            // We can't do a simple trim because the content might be terminated
            // with line separators we want to preserve.
            Ok((field_name, MultipartField::Text(text[..text.len() - 2].into())))
        }
    }

    /// Call `f` for each entry in the multipart request.
    /// This is a substitute for `Multipart` implementing `Iterator`,
    /// since `Iterator::next()` can't use bound lifetimes.
    ///
    /// See https://www.reddit.com/r/rust/comments/2lkk\4\isize/concrete_lifetime_vs_bound_lifetime/
    pub fn foreach_entry<F>(&mut self, mut foreach: F) where F: FnMut(String, MultipartField<R>) {
        loop {
            match self.read_entry() {
                Ok((name, field)) => foreach(name, field),
                Err(err) => {
                    error!("Error reading Multipart: {}", err);
                    break;
                },
            }
        }
    }

    fn read_content_disposition(&mut self) -> io::Result<(String, String, Option<String>)> {
        let line = try!(self.read_line());

        // Find the end of CONT_DISP in the line
        let disp_type = {
            const CONT_DISP: &'static str = "Content-Disposition:";

            let disp_idx = try_find!(CONT_DISP, &line, "Content-Disposition subheader not found!") 
                + CONT_DISP.len();

            let disp_type_end = try_find!(
                ';', &line[disp_idx..], 
                "Error parsing Content-Disposition value!"
            );

            line[disp_idx .. disp_idx + disp_type_end].trim().to_owned()
        };

        let field_name = {
            const NAME: &'static str = "name=\"";

            let name_idx = try_find!(NAME, &line, "Error parsing field name!") + NAME.len();
            let name_end = try_find!('"', &line[name_idx ..], "Error parsing field name!");

            line[name_idx .. name_idx + name_end].to_owned() // No trim here since it's in quotes.
        };

        let filename = {
            const FILENAME: &'static str = "filename=\"";

            let filename_idx = line.find(FILENAME).map(|idx| idx + FILENAME.len());
            let filename_idxs = with(filename_idx, |&start| line[start ..].find('"'));

            filename_idxs.map(|(start, end)| line[start .. start + end].to_owned())
        };

        Ok((disp_type, field_name, filename))
    }

    fn read_content_type(&mut self) -> io::Result<Option<String>> {
        debug!("Read content type!");
        let line = try!(self.read_line());

        const CONTENT_TYPE: &'static str = "Content-Type:";

        let type_idx = line.find(CONTENT_TYPE);

        // FIXME Will not properly parse for multiple files!
        // Does not expect boundary=<boundary>
        Ok(type_idx.map(|start| line[(start + CONTENT_TYPE.len())..].trim().to_owned()))
    }

    /// Read the request fully, parsing all fields and saving all files 
    /// to the given directory (if given) and return the result.
    ///
    /// If `dir` is none, uses `std::os::tmpdir()`.
    pub fn save_all(mut self, dir: Option<&Path>) -> io::Result<Entries> {
        let dir = dir.map_or_else(::std::env::temp_dir, |path| path.to_owned());

        let mut entries = Entries::with_path(dir);

        loop {
            match self.read_entry() {
                Ok((name, MultipartField::Text(text))) => { entries.fields.insert(name, text.to_owned()); },
                Ok((name, MultipartField::File(mut file))) => {
                    let path = try!(file.save_in(&entries.dir));
                    entries.files.insert(name, path);
                },
                Err(err) => {
                    error!("Error reading Multipart: {}", err);
                    break;
                },
            }
        }

        Ok(entries)
    }

    fn read_line(&mut self) -> io::Result<&str> {
        match self.source.read_line(&mut self.line_buf) {
            Ok(read) => Ok(&self.line_buf[..read]),
            Err(err) => Err(err),
        }
    }

    fn read_to_string(&mut self) -> io::Result<&str> {
        match self.source.read_to_string(&mut self.line_buf) {
            Ok(read) => Ok(&self.line_buf[..read]),
            Err(err) => Err(err),
        }
    }
}

impl<R> Deref for Multipart<R> where R: HttpRequest {
    type Target = R;
    fn deref(&self) -> &R {
        self.source.borrow()
    }
}

fn with<T, U, F: FnOnce(&T) -> Option<U>>(left: Option<T>, right: F) -> Option<(T, U)> {
    let temp = left.as_ref().and_then(right);
    match (left, temp) {
        (Some(lval), Some(rval)) => Some((lval, rval)),
        _ => None,
    }
}

fn line_error(msg: &str, line: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::Other,
        format!("Error: {:?} on line of request: {:?}", msg, line)
    )
}

/// A result of `Multipart::save_all()`.
pub struct Entries {
    pub fields: HashMap<String, String>,
    pub files: HashMap<String, PathBuf>,
    /// The directory the files were saved under.
    pub dir: PathBuf,
}

impl Entries {
    fn with_path<P: Into<PathBuf>>(path: P) -> Entries {
        Entries {
            fields: HashMap::new(),
            files: HashMap::new(),
            dir: path.into(),
        }
    }
}

pub trait HttpRequest: Read {
    fn is_multipart(&self) -> bool;
    fn get_boundary(&self) -> Option<&str>;
}

/// A field in a `multipart/form-data` request.
///
/// This enum does not include the names of the fields, as those are yielded separately
/// by `server::Multipart::read_entry()`.
#[derive(Debug)]
pub enum MultipartField<'a, R: 'a> {
    /// A text field.
    Text(&'a str),
    /// A file field, including the content type and optional filename
    /// along with a `Read` implementation for getting the contents.
    File(MultipartFile<'a, R>),
    // MultiFiles(Vec<MultipartFile>), /* TODO: Multiple files */
}

impl<'a, R> MultipartField<'a, R> {
    /// Borrow this field as a text field, if possible.
    pub fn as_text(&self) -> Option<&str> {
        match *self {
            MultipartField::Text(ref s) => Some(s),
            _ => None,
        }
    }

    /// Borrow this field as a file field, if possible
    /// Mutably borrows so the contents can be read.
    pub fn as_file(&mut self) -> Option<&mut MultipartFile<'a, R>> {
        match *self {
            MultipartField::File(ref mut file) => Some(file),
            _ => None,
        }
    }
}

/// A representation of a file in HTTP `multipart/form-data`.
///
/// Note that the file is not yet saved to the system; 
/// instead, the struct implements a `Reader` that points 
/// to the beginning of the file's contents in the HTTP stream. 
/// You can read it to EOF, or use one of the `save_*()` methods here 
/// to save it to disk.
#[derive(Debug)]
pub struct MultipartFile<'a, R: 'a> {
    filename: Option<String>,
    content_type: Mime,
    reader: &'a mut BoundaryReader<R>,
}

impl<'a, R: Read> MultipartFile<'a, R> {
    fn from_reader(
        filename: Option<String>, reader: &'a mut BoundaryReader<R>, cont_type: &str
    ) -> MultipartFile<'a, R> {
        MultipartFile {
            filename: filename,
            content_type: cont_type.parse::<Mime>().ok().unwrap_or_else(::mime_guess::octet_stream),            
            reader: reader,
        }    
    }

    /// Save this file to `path`, discarding the filename.
    ///
    /// If successful, the file can be found at `path`.
    pub fn save_as(&mut self, path: &Path) -> io::Result<()> {
        let mut file = try!(File::create(path));
        io::copy(self.reader, &mut file).and(Ok(()))
    }

    /// Save this file in the directory described by `dir`,
    /// appending `filename` if present, or a random string otherwise.
    ///
    /// Returns the created file's path on success.
    ///
    /// ###Panics
    /// If `dir` does not represent a directory.
    pub fn save_in(&mut self, dir: &Path) -> io::Result<PathBuf> {
        let meta = try!(fs::metadata(dir));
        assert!(meta.is_dir(), "Given path is not a directory!");

        let path = dir.join(::random_alphanumeric(8));
       
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

impl<'a, R: Read> Read for MultipartFile<'a, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>{
        self.reader.read(buf)
    }
}





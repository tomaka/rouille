//! The server-side implementation of `multipart/form-data` requests.
//!
//! Use this when you are implementing a server on top of Hyper and want to
//! to parse and serve POST `multipart/form-data` requests.
//!
//! See the `Multipart` struct for more info.

use mime::Mime;

use std::borrow::Borrow;
use std::ops::Deref;

use std::collections::HashMap;

use std::fs::{self, File};
use std::io;
use std::io::prelude::*;

use std::path::{Path, PathBuf};

#[doc(inline)]
pub use self::boundary::BoundaryReader;

mod boundary;

#[cfg(feature = "hyper")]
pub mod hyper;

macro_rules! try_opt (
    ($expr:expr) => (
        match $expr {
            Some(val) => val,
            None => return None,
        }
    )
);

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

impl<R> Multipart<R> where R: HttpRequest {
    /// If the given `HttpRequest` is a POST request of `Content-Type: multipart/form-data`,
    /// return the wrapped request as `Ok(Multipart)`, otherwise `Err(HttpRequest)`.
    pub fn from_request(req: R) -> Result<Multipart<R>, R> {
        if !req.is_multipart() { return Err(req); }

        if req.get_boundary().is_none() {
            return Err(req);     
        }

        let boundary = req.get_boundary().unwrap().to_owned();

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
    pub fn read_entry(&mut self) -> io::Result<Option<MultipartField<R>>> {
        MultipartField::read_from(self) 
    }

    fn read_content_disposition(&mut self) -> io::Result<Option<ContentDisp>> {
        let line = try!(self.read_line());
        Ok(ContentDisp::read_from(line))
    }

    /// Call `f` for each entry in the multipart request.
    /// This is a substitute for `Multipart` implementing `Iterator`,
    /// since `Iterator::next()` can't use bound lifetimes.
    ///
    /// See https://www.reddit.com/r/rust/comments/2lkk\4\isize/concrete_lifetime_vs_bound_lifetime/
    ///
    /// Returns `Ok(())` when all fields have been read, or the first error.
    pub fn foreach_entry<F>(&mut self, mut foreach: F) -> io::Result<()> where F: FnMut(MultipartField<R>) {
        loop {
            match self.read_entry() {
                Ok(Some(field)) => foreach(field),
                Ok(None) => return Ok(()),
                Err(err) => return Err(err),
            }
        }
    }

    fn read_content_type(&mut self) -> io::Result<Option<ContentType>> {
        debug!("Read content type!");
        let line = try!(self.read_line());
        Ok(ContentType::read_from(line))
    }

    /// Read the request fully, parsing all fields and saving all files 
    /// to the given directory or a random directory under `std::os::temp_dir()` 
    /// and return the result.
    ///
    /// If there is an error in reading the request, returns the result so far along with the
    /// error.
    pub fn save_all(&mut self, dir: Option<&Path>) -> (Entries, Option<io::Error>) {
        let dir = dir.map_or_else(::temp_dir, |path| path.to_owned());
        let mut entries = Entries::with_path(dir);

        loop {
            match self.read_entry() {
                Ok(Some(field)) => match field.data {
                    MultipartData::File(mut file) => {
                        entries.files.insert(field.name, file.save_in(&entries.dir));
                    },
                    MultipartData::Text(text) => {
                        entries.fields.insert(field.name, text.into());
                    },
                },
                Ok(None) => break,
                Err(err) => return (entries, Some(err)),
            }
        }

        (entries, None)
    }

    fn read_line(&mut self) -> io::Result<&str> {
        self.line_buf.clear();

        match self.source.read_line(&mut self.line_buf) {
            Ok(read) => Ok(&self.line_buf[..read]),
            Err(err) => Err(err),
        }
    }

    fn read_to_string(&mut self) -> io::Result<&str> {
        self.line_buf.clear();

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

struct ContentType {
    cont_type: Mime,
    #[allow(dead_code)]
    boundary: Option<String>,
}

impl ContentType {
    fn read_from(line: &str) -> Option<ContentType> {
        const CONTENT_TYPE: &'static str = "Content-Type:";
        const BOUNDARY: &'static str = "boundary=\"";

        debug!("Reading Content-Type header from line: {:?}", line);

        if let Some((cont_type, after_cont_type)) = get_str_after(CONTENT_TYPE, ';', line) {
            let cont_type = read_content_type(cont_type);

            let boundary = get_str_after(BOUNDARY, '"', after_cont_type).map(|tup| tup.0.into());

            Some(ContentType {
                cont_type: cont_type,
                boundary: boundary,
            })
        } else {
            get_remainder_after(CONTENT_TYPE, line).map(|cont_type| {
                let cont_type = read_content_type(cont_type);
                ContentType { cont_type: cont_type, boundary: None }
            })
        }
    }
}

fn read_content_type(cont_type: &str) -> Mime {
    cont_type.parse().ok().unwrap_or_else(::mime_guess::octet_stream)
}

struct ContentDisp {
    field_name: String,
    filename: Option<String>,
}

impl ContentDisp {
    fn read_from(line: &str) -> Option<ContentDisp> {
        debug!("Reading Content-Disposition from line: {:?}", line);

        if line.is_empty() {
            return None;
        }

        const CONT_DISP: &'static str = "Content-Disposition:";
        const NAME: &'static str = "name=\"";
        const FILENAME: &'static str = "filename=\"";

        let after_disp_type = {
            let (disp_type, after_disp_type) = try_opt!(get_str_after(CONT_DISP, ';', line));
            let disp_type = disp_type.trim();

            if disp_type != "form-data" {
                error!("Unexpected Content-Disposition value: {:?}", disp_type);
                return None;
            }

            after_disp_type
        };

        let (field_name, after_field_name) = try_opt!(get_str_after(NAME, '"', after_disp_type));

        let filename = get_str_after(FILENAME, '"', after_field_name)
            .map(|(filename, _)| filename.to_owned());

        Some(ContentDisp { field_name: field_name.to_owned(), filename: filename })
    }
}

/// Get the string after `needle` in `haystack`, stopping before `end_val_delim`
fn get_str_after<'a>(needle: &str, end_val_delim: char, haystack: &'a str) -> Option<(&'a str, &'a str)> {
    let val_start_idx = try_opt!(haystack.find(needle)) + needle.len();
    let val_end_idx = try_opt!(haystack[val_start_idx..].find(end_val_delim));
    Some((&haystack[val_start_idx..val_end_idx], &haystack[val_end_idx..]))
}

/// Get everything after `needle` in `haystack`
fn get_remainder_after<'a>(needle: &str, haystack: &'a str) -> Option<(&'a str)> {
    let val_start_idx = try_opt!(haystack.find(needle)) + needle.len();
    Some(&haystack[val_start_idx..])
}

pub trait HttpRequest: Read {
    fn is_multipart(&self) -> bool;
    fn get_boundary(&self) -> Option<&str>;
}

pub struct MultipartField<'a, R: 'a> {
    /// The field's name from the form
    pub name: String,
    /// The data of the field. Can be text or binary.
    pub data: MultipartData<'a, R>,
}

impl<'a, R: HttpRequest + 'a> MultipartField<'a, R> {
    fn read_from(multipart: &'a mut Multipart<R>) -> io::Result<Option<MultipartField<'a, R>>> {
        try!(multipart.source.consume_boundary());

        let cont_disp = match multipart.read_content_disposition() {
            Ok(Some(cont_disp)) => cont_disp,
            Ok(None) => return Ok(None),
            Err(err) => return Err(err),
        };        

        let data = match try!(multipart.read_content_type()) {
            Some(content_type) => {
                let _ = try!(multipart.read_line()); // Consume empty line
                MultipartData::File(
                    MultipartFile::from_reader(
                        cont_disp.filename, 
                        &mut multipart.source, 
                        content_type.cont_type,
                    )
                 )
            },
            None => {
                // Empty line consumed by read_content_type()
                let text = try!(multipart.read_to_string());
                // The last two characters are "\r\n".
                // We can't do a simple trim because the content might be terminated
                // with line separators we want to preserve.
                MultipartData::Text(&text[..text.len() - 2])
            },
        };

        Ok(Some(
            MultipartField {
                name: cont_disp.field_name,
                data: data,
            }
        ))
    }
} 

/// The data of a field in a `multipart/form-data` request.
#[derive(Debug)]
pub enum MultipartData<'a, R: 'a> {
    /// A text field.
    Text(&'a str),
    /// A file field, including the content type and optional filename
    /// along with a `Read` implementation for getting the contents.
    File(MultipartFile<'a, R>),
    // MultiFiles(Vec<MultipartFile>), /* TODO: Multiple files */
}

impl<'a, R> MultipartData<'a, R> {
    /// Borrow this field as a text field, if possible.
    pub fn as_text(&self) -> Option<&str> {
        match *self {
            MultipartData::Text(ref s) => Some(s),
            _ => None,
        }
    }

    /// Borrow this field as a file field, if possible
    /// Mutably borrows so the contents can be read.
    pub fn as_file(&mut self) -> Option<&mut MultipartFile<'a, R>> {
        match *self {
            MultipartData::File(ref mut file) => Some(file),
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
    pub filename: Option<String>,
    pub content_type: Mime,
    reader: &'a mut BoundaryReader<R>,
}

impl<'a, R: Read> MultipartFile<'a, R> {
    fn from_reader(
        filename: Option<String>, reader: &'a mut BoundaryReader<R>, cont_type: Mime, 
    ) -> MultipartFile<'a, R> {
        MultipartFile {
            filename: filename,
            content_type: cont_type,            
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

/// A result of `Multipart::save_all()`.
pub struct Entries {
    /// The text files of the multipart request
    pub fields: HashMap<String, String>,
    /// A list of file field names and their save results.
    pub files: HashMap<String, io::Result<PathBuf>>,
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


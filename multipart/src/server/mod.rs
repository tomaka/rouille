//! The server-side abstraction for multipart requests. Enabled with the `server` feature (on by
//! default).
//!
//! Use this when you are implementing an HTTP server and want to
//! to accept, parse, and serve HTTP `multipart/form-data` requests (file uploads).
//!
//! See the `Multipart` struct for more info.

use mime::Mime;

use std::borrow::Borrow;

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
/// Implements `Borrow<R>` to allow access to the request object.
pub struct Multipart<R> {
    source: BoundaryReader<R>,
    line_buf: String,
    at_end: bool,    
    /// The directory for saving files in this request.
    /// By default, this is set to a subdirectory of `std::env::temp_dir()` with a 
    /// random alphanumeric name.
    pub save_dir: PathBuf,
}

impl<R> Multipart<R> where R: HttpRequest {
    /// If the given `R: HttpRequest` is a POST request of `Content-Type: multipart/form-data`,
    /// return the wrapped request as `Ok(Multipart<R>)`, otherwise `Err(R)`.
    pub fn from_request(req: R) -> Result<Multipart<R>, R> {
        if !req.is_multipart() { return Err(req); }

        if req.boundary().is_none() {
            return Err(req);     
        }

        let boundary = format!("--{}", req.boundary().unwrap());

        debug!("Boundary: {}", boundary);

        Ok(
            Multipart { 
                source: BoundaryReader::from_reader(req, boundary),
                line_buf: String::new(),
                at_end: false,                
                save_dir: ::temp_dir(),
            }
        )
    }

    /// Read the next entry from this multipart request, returning a struct with the field's name and
    /// data. See `MultipartField` for more info.
    ///
    /// ##Warning: Risk of Data Loss
    /// If the previously returned entry had contents of type `MultipartField::File`,
    /// calling this again will discard any unread contents of that entry.
    pub fn read_entry(&mut self) -> io::Result<Option<MultipartField<R>>> {
        if self.at_end { return Ok(None); }

        try!(self.source.consume_boundary());

        self.at_end = {
            try!(self.read_line()) == "--\r\n"
        };
        
        if !self.at_end {
            MultipartField::read_from(self)
        } else {
            Ok(None)
        }
    }

    fn read_content_disposition(&mut self) -> io::Result<Option<ContentDisp>> {
        let line = try!(self.read_line());
        Ok(ContentDisp::read_from(line))
    }

    /// Call `f` for each entry in the multipart request.
    /// 
    /// This is a substitute for Rust not supporting streaming iterators (where the return value
    /// from `next()` borrows the iterator for a bound lifetime).
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

    /// Read the request fully, parsing all fields and saving all files in `self.save_dir`. 
    ///
    /// If there is an error in reading the request, returns the partial result along with the
    /// error.
    pub fn save_all(&mut self) -> (Entries, Option<io::Error>) {
        let mut entries = Entries::with_path(self.save_dir.clone());

        loop {
            match self.read_entry() {
                Ok(Some(field)) => match field.data {
                    MultipartData::File(mut file) => {
                        entries.files.insert(field.name, file.save());
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

impl<R> Borrow<R> for Multipart<R> where R: HttpRequest {
    fn borrow(&self) -> &R {
        self.source.borrow()
    }
}

struct ContentType {
    val: Mime,
    #[allow(dead_code)]
    boundary: Option<String>,
}

impl ContentType {
    fn read_from(line: &str) -> Option<ContentType> {
        const CONTENT_TYPE: &'static str = "Content-Type:";
        const BOUNDARY: &'static str = "boundary=\"";

        debug!("Reading Content-Type header from line: {:?}", line);

        if let Some((cont_type, after_cont_type)) = get_str_after(CONTENT_TYPE, ';', line) {
            let content_type = read_content_type(cont_type.trim());

            let boundary = get_str_after(BOUNDARY, '"', after_cont_type).map(|tup| tup.0.into());

            Some(ContentType {
                val: content_type,
                boundary: boundary,
            })
        } else {
            get_remainder_after(CONTENT_TYPE, line).map(|cont_type| {
                let content_type = read_content_type(cont_type.trim());
                ContentType { val: content_type, boundary: None }
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
    let val_end_idx = try_opt!(haystack[val_start_idx..].find(end_val_delim)) + val_start_idx;
    Some((&haystack[val_start_idx..val_end_idx], &haystack[val_end_idx..]))
}

/// Get everything after `needle` in `haystack`
fn get_remainder_after<'a>(needle: &str, haystack: &'a str) -> Option<(&'a str)> {
    let val_start_idx = try_opt!(haystack.find(needle)) + needle.len();
    Some(&haystack[val_start_idx..])
}

/// A server-side HTTP request that may or may not be multipart.
pub trait HttpRequest: Read {
    /// Return `true` if this request is a `multipart/form-data` request, `false` otherwise.
    fn is_multipart(&self) -> bool;
    /// Get the boundary string of this request if it is `multipart/form-data`.
    fn boundary(&self) -> Option<&str>;
}

/// A field in a multipart request. May be either text or a binary stream (file).
#[derive(Debug)]
pub struct MultipartField<'a, R: 'a> {
    /// The field's name from the form
    pub name: String,
    /// The data of the field. Can be text or binary.
    pub data: MultipartData<'a, R>,
}

impl<'a, R: HttpRequest + 'a> MultipartField<'a, R> {
    fn read_from(multipart: &'a mut Multipart<R>) -> io::Result<Option<MultipartField<'a, R>>> {
        let cont_disp = match multipart.read_content_disposition() {
            Ok(Some(cont_disp)) => cont_disp,
            Ok(None) => return Ok(None),
            Err(err) => return Err(err),
        };        

        let data = match try!(multipart.read_content_type()) {
            Some(content_type) => {
                let _ = try!(multipart.read_line()); // Consume empty line
                MultipartData::File(
                    MultipartFile::from_stream(
                        cont_disp.filename, 
                        content_type.val,
                        &multipart.save_dir,
                        &mut multipart.source,
                    )
                 )
            },
            None => {
                // Empty line consumed by read_content_type()
                let text = try!(multipart.read_to_string());
                // The last two characters are "\r\n".
                // We can't do a simple trim because the content might be terminated
                // with line separators we want to preserve.
                MultipartData::Text(&text[..text.len()])
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
    /// The field's payload is a text string.
    Text(&'a str),
    /// The field's payload is a binary stream (file).
    File(MultipartFile<'a, R>),
    // TODO: Support multiple files per field (nested boundaries)
    // MultiFiles(Vec<MultipartFile>),
}

impl<'a, R> MultipartData<'a, R> {
    /// Borrow this payload as a text field, if possible.
    pub fn as_text(&self) -> Option<&str> {
        match *self {
            MultipartData::Text(ref s) => Some(s),
            _ => None,
        }
    }

    /// Borrow this payload as a file field, if possible.
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
/// instead, this struct exposes `Read` and `BufRead` impls which point
/// to the beginning of the file's contents in the HTTP stream. 
///
/// You can read it to EOF, or use one of the `save_*()` methods here 
/// to save it to disk.
#[derive(Debug)]
pub struct MultipartFile<'a, R: 'a> {
    filename: Option<String>,
    content_type: Mime,
    save_dir: &'a Path,
    stream: &'a mut BoundaryReader<R>,
}

impl<'a, R: Read> MultipartFile<'a, R> {
    fn from_stream(filename: Option<String>, 
                   content_type: Mime, 
                   save_dir: &'a Path,
                   stream: &'a mut BoundaryReader<R>) -> MultipartFile<'a, R> {
        MultipartFile {
            filename: filename,
            content_type: content_type,
            save_dir: save_dir,
            stream: stream,
        }    
    }

    /// Save this file to `path`.
    ///
    /// Returns the number of bytes written on success, or any errors otherwise.
    ///
    /// Retries when `io::Error::kind() == io::ErrorKind::Interrupted`.
    pub fn save_as(&mut self, path: &Path) -> io::Result<u64> {
        let mut file = try!(File::create(path));
        retry_on_interrupt(|| io::copy(self.stream, &mut file))    
    }

    /// Save this file in the directory pointed at by `dir`,
    /// using `self.filename()` if present, or a random alphanumeric string otherwise.
    ///
    /// Any missing directories in the `dir` path will be created.
    ///
    /// `self.filename()` is sanitized of all file separators before being appended to `dir`.
    ///
    /// Returns the created file's path on success, or any errors otherwise.
    ///
    /// Retries when `io::Error::kind() == io::ErrorKind::Interrupted`.
    pub fn save_in(&mut self, dir: &Path) -> io::Result<PathBuf> {
        try!(fs::create_dir_all(dir));
        let path = self.gen_safe_file_path(dir);
        self.save_as(&path).map(move |_| path)
    }

    /// Save this file in the directory pointed at by `self.save_dir`,
    /// using `self.filename()` if present, or a random alphanumeric string otherwise.
    ///
    /// Any missing directories in the `self.save_dir` path will be created.
    ///
    /// `self.filename()` is sanitized of all file separators before being appended to `self.save_dir`.
    ///
    /// Returns the created file's path on success, or any errors otherwise.
    ///
    /// Retries when `io::Error::kind() == io::ErrorKind::Interrupted`.
    pub fn save(&mut self) -> io::Result<PathBuf> {
        try!(fs::create_dir_all(self.save_dir));
        let path = self.gen_safe_file_path(self.save_dir);
        self.save_as(&path).map(move |_| path)
    }

    /// Get the filename of this entry, if supplied.
    ///
    /// ##Warning
    /// You should treat this value as untrustworthy because it is an arbitrary string provided by
    /// the client. You should *not* blindly append it to a directory path and save the file there, 
    /// as such behavior could easily be exploited by a malicious client.
    pub fn filename(&self) -> Option<&str> {
        self.filename.as_ref().map(String::as_ref)    
    }

    /// Get the MIME type (`Content-Type` value) of this file, if supplied by the client, 
    /// or `"applicaton/octet-stream"` otherwise.
    pub fn content_type(&self) -> Mime {
        self.content_type.clone()    
    }

    /// The save directory assigned to this file field by the `Multipart` instance it was read
    /// from.
    pub fn save_dir(&self) -> &Path {
        self.save_dir
    }

    fn gen_safe_file_path(&self, dir: &Path) -> PathBuf { 
        self.filename().map(Path::new)
            .and_then(Path::file_name) //Make sure there's no path separators in the filename
            .map_or_else(
                || dir.join(::random_alphanumeric(8)), 
                |filename| dir.join(filename),
            )
    }
}

impl<'a, R: Read> Read for MultipartFile<'a, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>{
        self.stream.read(buf)
    }
}

impl<'a, R: Read> BufRead for MultipartFile<'a, R> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.stream.fill_buf()
    }

    fn consume(&mut self, amt: usize) {
        self.stream.consume(amt)
    }
}

/// A result of `Multipart::save_all()`.
pub struct Entries {
    /// The text fields of the multipart request.
    pub fields: HashMap<String, String>,
    /// A map of file field names to their save results.
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

fn retry_on_interrupt<F, T>(mut do_fn: F) -> io::Result<T> where F: FnMut() -> io::Result<T> {
    loop {
        match do_fn() {
            Ok(val) => return Ok(val),
            Err(err) => if err.kind() != io::ErrorKind::Interrupted { 
                return Err(err);
            },
        }
    }
} 


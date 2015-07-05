//! The server-side implementation of `multipart/form-data` requests.
//!
//! Use this when you are implementing a server on top of Hyper and want to
//! to parse and serve POST `multipart/form-data` requests.
//!
//! See the `Multipart` struct for more info.

use mime::{Mime, TopLevel, SubLevel, Attr, Value};

use super::{MultipartField, MultipartFile};

use std::borrow::Borrow;
use std::cmp;
use std::collections::HashMap;
use std::ops::Deref;

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

        let boundary = match req.get_boundary() {
            Some(boundary) => format!("{}\r\n", boundary),
            None => return Err(req),        
        };

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
    pub fn read_entry(&mut self) -> io::Result<(String, MultipartField)> {
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
                    MultipartFile::from_octet(filename, &mut self.source, &content_type)
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
    pub fn foreach_entry<F>(&mut self, mut foreach: F) where F: FnMut(String, MultipartField) {
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
    /// If `dir` is none, chooses a random subdirectory under `std::os::tmpdir()`.
    pub fn save_all(mut self, dir: Option<&Path>) -> io::Result<Entries> {
        let tmp_dir = super::random_alphanumeric(12);
        let dir = dir.map_or_else(|| ::std::env::temp_dir().join(tmp_dir), |path| path.to_owned());

        let mut entries = Entries::with_path(dir);

        loop {
            match self.read_entry() {
                Ok((name, MultipartField::Text(text))) => { entries.fields.insert(name, text.into_owned()); },
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
        self.source.read_line(&mut self.line_buf).map(|read| &self.line_buf[..read])
    }

    fn read_to_string(&mut self) -> io::Result<&str> {
        self.source.read_to_string(&mut self.line_buf).map(|read| &self.line_buf[..read])
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

/* FIXME: Can't have an iterator return a borrowed reference
impl<'a> Iterator<(String, MultipartField<'a>)> for Multipart<'a> {
    fn next(&mut self) -> Option<(String, MultipartField<'a>)> {
        match self.read_entry() {
            Ok(ok) => Some(ok),
            Err(err) => {
                if err.kind != EndOfFile {
                    error!("Error reading Multipart: {}", err);
                }

                None
             },
        }
    }
}
*/

pub trait HttpRequest: Read {
    fn is_multipart(&self) -> bool;
    fn get_boundary(&self) -> Option<&str>;
}




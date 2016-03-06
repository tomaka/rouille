// Copyright 2016 `multipart` Crate Developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.
//! The server-side abstraction for multipart requests. Enabled with the `server` feature (on by
//! default).
//!
//! Use this when you are implementing an HTTP server and want to
//! to accept, parse, and serve HTTP `multipart/form-data` requests (file uploads).
//!
//! See the `Multipart` struct for more info.
use mime::Mime;

use tempdir::TempDir;

use std::borrow::Borrow;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use std::{fmt, io, mem, ptr};

use self::boundary::BoundaryReader;

macro_rules! try_opt (
    ($expr:expr) => (
        match $expr {
            Some(val) => val,
            None => return None,
        }
    )
);

mod boundary;

#[cfg(feature = "hyper")]
pub mod hyper;

#[cfg(feature = "iron")]
pub mod iron;

#[cfg(feature = "nickel2")]
mod nickel;

#[cfg(feature = "tiny_http")]
pub mod tiny_http;

const RANDOM_FILENAME_LEN: usize = 12;

/// The server-side implementation of `multipart/form-data` requests.
///
/// Implements `Borrow<R>` to allow access to the request body, if desired.
pub struct Multipart<B> {
    source: BoundaryReader<B>,
    line_buf: String, 
}

impl Multipart<()> {
    /// If the given `HttpRequest` is a multipart/form-data POST request,
    /// return the request body wrapped in the multipart reader. Otherwise,
    /// returns the original request.
    pub fn from_request<R: HttpRequest>(req: R) -> Result<Multipart<R::Body>, R> {
        //FIXME: move `map` expr to `Some` arm when nonlexical borrow scopes land.
        let boundary = match req.multipart_boundary().map(String::from) {
            Some(boundary) => boundary,
            None => return Err(req),
        };

        Ok(Multipart::with_body(req.body(), boundary))        
    }   
}

impl<B: Read> Multipart<B> {
    /// Construct a new `Multipart` with the given body reader and boundary.
    /// This will prepend the requisite `"--"` to the boundary.
    pub fn with_body<Bnd: Into<String>>(body: B, boundary: Bnd) -> Self {
        let boundary = prepend_str("--", boundary.into());

        debug!("Boundary: {}", boundary);

        Multipart { 
            source: BoundaryReader::from_reader(body, boundary),
            line_buf: String::new(),
        }
    }

    /// Read the next entry from this multipart request, returning a struct with the field's name and
    /// data. See `MultipartField` for more info.
    ///
    /// ##Warning: Risk of Data Loss
    /// If the previously returned entry had contents of type `MultipartField::File`,
    /// calling this again will discard any unread contents of that entry.
    pub fn read_entry(&mut self) -> io::Result<Option<MultipartField<B>>> {
        if !try!(self.consume_boundary()) {
            return Ok(None);
        }

        MultipartField::read_from(self)
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
    pub fn foreach_entry<F>(&mut self, mut foreach: F) -> io::Result<()> where F: FnMut(MultipartField<B>) {
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

    /// Read the request fully, parsing all fields and saving all files in a new temporary
    /// directory under the OS temporary directory. 
    ///
    /// If there is an error in reading the request, returns the partial result along with the
    /// error. See [`SaveResult`](enum.saveresult.html) for more information.
    pub fn save_all(&mut self) -> SaveResult {
        let mut entries = match Entries::new_tempdir() {
            Ok(entries) => entries,
            Err(err) => return SaveResult::Error(err),
        };
 
        match self.read_to_entries(&mut entries) {
            Ok(()) => SaveResult::Full(entries),
            Err(err) => SaveResult::Partial(entries, err),
        }
    }

    /// Read the request fully, parsing all fields and saving all files in a new temporary
    /// directory under `dir`. 
    ///
    /// If there is an error in reading the request, returns the partial result along with the
    /// error. See [`SaveResult`](enum.saveresult.html) for more information.
    pub fn save_all_under<P: AsRef<Path>>(&mut self, dir: P) -> SaveResult {
        let mut entries = match Entries::new_tempdir_in(dir) {
            Ok(entries) => entries,
            Err(err) => return SaveResult::Error(err),
        };

        match self.read_to_entries(&mut entries) {
            Ok(()) => SaveResult::Full(entries),
            Err(err) => SaveResult::Partial(entries, err),
        }
    }

    fn read_to_entries(&mut self, entries: &mut Entries) -> io::Result<()> {
        while let Some(field) = try!(self.read_entry()) {
            match field.data {
                MultipartData::File(mut file) => {
                    let file = try!(file.save_in(&entries.dir));
                    entries.files.insert(field.name, file);
                },
                MultipartData::Text(text) => {
                    entries.fields.insert(field.name, text.into());
                },
            }
        }

        Ok(())
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

    fn consume_boundary(&mut self) -> io::Result<bool> {
        try!(self.source.consume_boundary());

        let mut out = [0; 2];
        let _ = try!(self.source.read(&mut out));

        if *b"\r\n" == out {
            return Ok(true);
        } else {
            if *b"--" != out {
                warn!("Unexpected 2-bytes after boundary: {:?}", out);
            }

            return Ok(false);
        }
    }
}

impl<B> Borrow<B> for Multipart<B> {
    fn borrow(&self) -> &B {
        self.source.borrow()
    }
}

/// The result of [`Multipart::save_all()`](struct.multipart.html#method.save_all).
#[derive(Debug)]
pub enum SaveResult {
    /// The operation was a total success. Contained are all entries of the request.
    Full(Entries),
    /// The operation errored partway through. Contained are the entries gathered thus far,
    /// as well as the error that ended the process.
    Partial(Entries, io::Error),
    /// The `TempDir` for `Entries` could not be constructed. Contained is the error detailing the
    /// problem.
    Error(io::Error),
}

impl SaveResult {
    /// Take the `Entries` from `self`, if applicable, and discarding
    /// the error, if any.
    pub fn to_entries(self) -> Option<Entries> {
        use self::SaveResult::*;

        match self {
            Full(entries) | Partial(entries, _) => Some(entries),
            Error(_) => None,
        }
    }

    /// Decompose `self` to `(Option<Entries>, Option<io::Error>)`
    pub fn to_opt(self) -> (Option<Entries>, Option<io::Error>) {
        use self::SaveResult::*;

        match self {
            Full(entries) => (Some(entries), None),
            Partial(entries, error) => (Some(entries), Some(error)),
            Error(error) => (None, Some(error)),
        }
    }

    /// Map `self` to an `io::Result`, discarding the error in the `Partial` case.
    pub fn to_result(self) -> io::Result<Entries> {
        use self::SaveResult::*;

        match self {
            Full(entries) | Partial(entries, _) => Ok(entries),
            Error(error) => Err(error),
        }
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
///
/// May be implemented by mutable references if providing the request or body by-value is
/// undesirable.
pub trait HttpRequest {
    /// The body of this request.
    type Body: Read;
    /// Get the boundary string of this request if it is a POST request
    /// with the `Content-Type` header set to `multipart/form-data`.
    ///
    /// The boundary string should be supplied as an extra value of the `Content-Type` header, e.g.
    /// `Content-Type: multipart/form-data; boundary={boundary}`.
    fn multipart_boundary(&self) -> Option<&str>;

    /// Return the request body for reading.
    fn body(self) -> Self::Body;
}

/// A field in a multipart request. May be either text or a binary stream (file).
#[derive(Debug)]
pub struct MultipartField<'a, B: 'a> {
    /// The field's name from the form
    pub name: String,
    /// The data of the field. Can be text or binary.
    pub data: MultipartData<'a, B>,
}

impl<'a, B: Read + 'a> MultipartField<'a, B> {
    fn read_from(multipart: &'a mut Multipart<B>) -> io::Result<Option<MultipartField<'a, B>>> {
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
                        &mut multipart.source,
                    )
                 )
            },
            None => {
                // Empty line consumed by read_content_type()
                let text = try!(multipart.read_to_string()); 
                MultipartData::Text(&text)
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
pub enum MultipartData<'a, B: 'a> {
    /// The field's payload is a text string.
    Text(&'a str),
    /// The field's payload is a binary stream (file).
    File(MultipartFile<'a, B>),
    // TODO: Support multiple files per field (nested boundaries)
    // MultiFiles(Vec<MultipartFile>),
}

impl<'a, B> MultipartData<'a, B> {
    /// Borrow this payload as a text field, if possible.
    pub fn as_text(&self) -> Option<&str> {
        match *self {
            MultipartData::Text(ref s) => Some(s),
            _ => None,
        }
    }

    /// Borrow this payload as a file field, if possible.
    /// Mutably borrows so the contents can be read.
    pub fn as_file(&mut self) -> Option<&mut MultipartFile<'a, B>> {
        match *self {
            MultipartData::File(ref mut file) => Some(file),
            _ => None,
        }
    }
}

/// A representation of a file in HTTP `multipart/form-data`.
///
/// Note that the file is not yet saved to the local filesystem; 
/// instead, this struct exposes `Read` and `BufRead` impls which point
/// to the beginning of the file's contents in the HTTP stream. 
///
/// You can read it to EOF, or use one of the `save_*()` methods here 
/// to save it to disk.
#[derive(Debug)]
pub struct MultipartFile<'a, B: 'a> {
    filename: Option<String>,
    content_type: Mime,
    stream: &'a mut BoundaryReader<B>,
}

impl<'a, B: Read> MultipartFile<'a, B> {
    fn from_stream(filename: Option<String>, 
                   content_type: Mime, 
                   stream: &'a mut BoundaryReader<B>) -> MultipartFile<'a, B> {
        MultipartFile {
            filename: filename,
            content_type: content_type,
            stream: stream,
        }    
    }

    /// Save this file to the given output stream.
    ///
    /// If successful, returns the number of bytes written.
    ///
    /// Retries when `io::Error::kind() == io::ErrorKind::Interrupted`.
    pub fn save_to<W: Write>(&mut self, mut out: W) -> io::Result<u64> {
        retry_on_interrupt(|| io::copy(self.stream, &mut out))
    }

    /// Save this file to the given output stream, **truncated** to `limit` 
    /// (no more than `limit` bytes will be written out).
    ///
    /// If successful, returns the number of bytes written.
    ///
    /// Retries when `io::Error::kind() == io::ErrorKind::Interrupted`.
    pub fn save_to_limited<W: Write>(&mut self, mut out: W, limit: u64) -> io::Result<u64> {
        retry_on_interrupt(|| io::copy(&mut self.stream.take(limit), &mut out))
    }

    /// Save this file to `path`.
    ///
    /// Returns the saved file info on success, or any errors otherwise.
    ///
    /// Retries when `io::Error::kind() == io::ErrorKind::Interrupted`.
    pub fn save_as<P: Into<PathBuf>>(&mut self, path: P) -> io::Result<SavedFile> {
        let path = path.into();
        let file = try!(create_full_path(&path)); 
        let size = try!(self.save_to(file));

        Ok(SavedFile {
            path: path,
            filename: self.filename.clone(),
            size: size,
        })
    }

    /// Save this file in the directory pointed at by `dir`,
    /// using a random alphanumeric string as the filename.
    ///
    /// Any missing directories in the `dir` path will be created.
    ///
    /// Returns the saved file's info on success, or any errors otherwise.
    ///
    /// Retries when `io::Error::kind() == io::ErrorKind::Interrupted`.
    pub fn save_in<P: AsRef<Path>>(&mut self, dir: P) -> io::Result<SavedFile> {
        let path = dir.as_ref().join(::random_alphanumeric(RANDOM_FILENAME_LEN));
        self.save_as(path)
    }

    /// Save this file to `path`, **truncated** to `limit` (no more than `limit` bytes will be written out).
    ///
    /// Any missing directories in the `dir` path will be created.
    ///
    /// Returns the saved file's info on success, or any errors otherwise.
    ///
    /// Retries when `io::Error::kind() == io::ErrorKind::Interrupted`.
    pub fn save_as_limited<P: Into<PathBuf>>(&mut self, path: P, limit: u64) -> io::Result<SavedFile> {
        let path = path.into();
        let file = try!(create_full_path(&path));
        let size = try!(self.save_to_limited(file, limit));
        
        Ok(SavedFile {
            path: path,
            filename: self.filename.clone(),
            size: size,
        })
    }
    
    /// Save this file in the directory pointed at by `dir`,
    /// using a random alphanumeric string as the filename.
    ///
    /// **Truncates** file to `limit` (no more than `limit` bytes will be written out).
    ///
    /// Any missing directories in the `dir` path will be created.
    ///
    /// Returns the saved file's info on success, or any errors otherwise.
    ///
    /// Retries when `io::Error::kind() == io::ErrorKind::Interrupted`.
    pub fn save_in_limited<P: AsRef<Path>>(&mut self, dir: P, limit: u64) -> io::Result<SavedFile> {
        let path = dir.as_ref().join(::random_alphanumeric(RANDOM_FILENAME_LEN));
        self.save_as_limited(path, limit)
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
    pub fn content_type(&self) -> &Mime {
        &self.content_type    
    }
}

impl<'a, B: Read> Read for MultipartFile<'a, B> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>{
        self.stream.read(buf)
    }
}

impl<'a, B: Read> BufRead for MultipartFile<'a, B> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.stream.fill_buf()
    }

    fn consume(&mut self, amt: usize) {
        self.stream.consume(amt)
    }
}

/// A result of `Multipart::save_all()`.
#[derive(Debug)]
pub struct Entries {
    /// The text fields of the multipart request, mapped by field name -> value.
    pub fields: HashMap<String, String>,
    /// A map of file field names to their contents saved on the filesystem.
    pub files: HashMap<String, SavedFile>,
    /// The directory the files in this request were saved under; may be temporary or permanent.
    pub dir: SaveDir,
}

impl Entries {
    fn new_tempdir_in<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        TempDir::new_in(path, "multipart").map(Self::with_tempdir)
    }

    fn new_tempdir() -> io::Result<Self> {
        TempDir::new("multipart").map(Self::with_tempdir)
    }

    fn with_tempdir(tempdir: TempDir) -> Entries {
        Entries {
            fields: HashMap::new(),
            files: HashMap::new(),
            dir: SaveDir::Temp(tempdir),
        }
    }
}

/// The save directory for `Entries`. May be temporary (delete-on-drop) or permanent.
pub enum SaveDir {
    /// This directory is temporary and will be deleted, along with its contents, when this wrapper
    /// is dropped.
    Temp(TempDir),
    /// This directory is permanent and will be left on the filesystem when this wrapper is dropped.
    Perm(PathBuf),
}

impl SaveDir {
    /// Get the path of this directory, either temporary or permanent.
    pub fn as_path(&self) -> &Path {
        use self::SaveDir::*;
        match *self {
            Temp(ref tempdir) => tempdir.path(),
            Perm(ref pathbuf) => &*pathbuf,
        }
    }

    /// Returns `true` if this is a temporary directory which will be deleted on-drop.
    pub fn is_temporary(&self) -> bool {
        use self::SaveDir::*;
        match *self {
            Temp(_) => true,
            Perm(_) => false,
        }
    }

    /// Unwrap the `PathBuf` from `self`; if this is a temporary directory,
    /// it will be converted to a permanent one.
    pub fn into_path(self) -> PathBuf {
        use self::SaveDir::*;

        match self {
            Temp(tempdir) => tempdir.into_path(),
            Perm(pathbuf) => pathbuf,
        }
    }

    /// If this `SaveDir` is temporary, convert it to permanent.
    /// This is a no-op if it already is permanent.
    ///
    /// ###Warning: Potential Data Loss
    /// Even though this will prevent deletion on-drop, the temporary folder on most OSes
    /// (where this directory is created by default) can be automatically cleared by the OS at any
    /// time, usually on reboot or when free space is low.
    ///
    /// It is recommended that you relocate the files from a request which you want to keep to a 
    /// permanent folder on the filesystem.
    pub fn keep(&mut self) {
        use self::SaveDir::*;
        *self = match mem::replace(self, Perm(PathBuf::new())) {
            Temp(tempdir) => Perm(tempdir.into_path()),
            old_self => old_self,
        };
    }

    /// Delete this directory and its contents, regardless of its permanence.
    ///
    /// ###Warning: Potential Data Loss
    /// This is very likely irreversible, depending on the OS implementation.
    ///
    /// Files deleted programmatically are deleted directly from disk, as compared to most file
    /// manager applications which use a staging area from which deleted files can be safely
    /// recovered (i.e. Windows' Recycle Bin, OS X's Trash Can, etc.).
    pub fn delete(self) -> io::Result<()> {
        use self::SaveDir::*;
        match self {
            Temp(tempdir) => tempdir.close(),
            Perm(pathbuf) => fs::remove_dir_all(&pathbuf),
        }
    }
}

impl AsRef<Path> for SaveDir {
    fn as_ref(&self) -> &Path {
        self.as_path()
    }
}

// grrr, no Debug impl for TempDir, can't derive
// FIXME when tempdir > 0.3.4 is released (Debug PR landed 3/3/2016) 
impl fmt::Debug for SaveDir {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::SaveDir::*;

        match *self {
            Temp(ref tempdir) => write!(f, "SaveDir::Temp({:?})", tempdir.path()),
            Perm(ref path) => write!(f, "SaveDir::Perm({:?})", path),
        }
    }
}

/// A file saved to the local filesystem from a multipart request.
#[derive(Debug)]
pub struct SavedFile {
    /// The complete path this file was saved at.
    pub path: PathBuf,

    /// The original filename of this file, if one was provided in the request.
    ///
    /// ##Warning
    /// You should treat this value as untrustworthy because it is an arbitrary string provided by
    /// the client. You should *not* blindly append it to a directory path and save the file there, 
    /// as such behavior could easily be exploited by a malicious client.
    pub filename: Option<String>,

    /// The number of bytes written to the disk; may be truncated.
    pub size: u64,
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

fn prepend_str(prefix: &str, mut string: String) -> String {
    string.reserve(prefix.len());

    unsafe {
        let bytes = string.as_mut_vec();

        // This addition is safe because it was already done in `String::reserve()`
        // which would have panicked if it overflowed.
        let old_len = bytes.len();
        let new_len = bytes.len() + prefix.len();
        bytes.set_len(new_len);

        ptr::copy(bytes.as_ptr(), bytes[prefix.len()..].as_mut_ptr(), old_len);
        ptr::copy(prefix.as_ptr(), bytes.as_mut_ptr(), prefix.len());
    }

    string
}

fn create_full_path(path: &Path) -> io::Result<File> {
    if let Some(parent) = path.parent() {
        try!(fs::create_dir_all(parent));
    } else {
        // RFC: return an error instead?
        warn!("Attempting to save file in what looks like a root directory. File path: {:?}", path);
    }

    File::create(&path)
}

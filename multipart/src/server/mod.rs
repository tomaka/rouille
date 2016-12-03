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

extern crate buf_redux;
extern crate httparse;
extern crate memchr;

use tempdir::TempDir;

use std::borrow::Borrow;
use std::collections::HashMap;
use std::fs;
use std::io::prelude::*;
use std::path::{Path, PathBuf};
use std::{io, mem, ptr};

use self::boundary::BoundaryReader;
use self::field::FieldHeaders;

pub use self::field::{MultipartField, MultipartFile, MultipartData, SavedFile};

macro_rules! try_opt (
    ($expr:expr) => (
        match $expr {
            Some(val) => val,
            None => return None,
        }
    );
    ($expr:expr, $before_ret:expr) => (
        match $expr {
            Some(val) => val,
            None => {
                $before_ret;
                return None;
            }
        }
    )
);

mod boundary;
mod field;

#[cfg(feature = "hyper")]
pub mod hyper;

#[cfg(feature = "iron")]
pub mod iron;

#[cfg(feature = "nickel")]
pub mod nickel;

#[cfg(feature = "tiny_http")]
pub mod tiny_http;

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
        if try!(self.consume_boundary()) {
            return Ok(None);
        }

        self::field::read_field(self)
    }

    fn read_field_headers(&mut self) -> io::Result<Option<FieldHeaders>> {
        FieldHeaders::parse(&mut self.source)
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
 
        match self.read_to_entries(&mut entries, None) {
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

        match self.read_to_entries(&mut entries, None) {
            Ok(()) => SaveResult::Full(entries),
            Err(err) => SaveResult::Partial(entries, err),
        }
    }

    /// Read the request fully, parsing all fields and saving all fields in a new temporary
    /// directory under the OS temporary directory.
    ///
    /// Files larger than `limit` will be truncated to `limit`.
    ///
    /// If there is an error in reading the request, returns the partial result along with the
    /// error. See [`SaveResult`](enum.saveresult.html) for more information.
    pub fn save_all_limited(&mut self, limit: u64) -> SaveResult {
        let mut entries = match Entries::new_tempdir() {
            Ok(entries) => entries,
            Err(err) => return SaveResult::Error(err),
        };

        match self.read_to_entries(&mut entries, Some(limit)) {
            Ok(()) => SaveResult::Full(entries),
            Err(err) => SaveResult::Partial(entries, err),
        }
    }

    /// Read the request fully, parsing all fields and saving all files in a new temporary
    /// directory under `dir`. 
    ///
    /// Files larger than `limit` will be truncated to `limit`.
    ///
    /// If there is an error in reading the request, returns the partial result along with the
    /// error. See [`SaveResult`](enum.saveresult.html) for more information.
    pub fn save_all_under_limited<P: AsRef<Path>>(&mut self, dir: P, limit: u64) -> SaveResult {
        let mut entries = match Entries::new_tempdir_in(dir) {
            Ok(entries) => entries,
            Err(err) => return SaveResult::Error(err),
        };

        match self.read_to_entries(&mut entries, Some(limit)) {
            Ok(()) => SaveResult::Full(entries),
            Err(err) => SaveResult::Partial(entries, err),
        }
    }

    fn read_to_entries(&mut self, entries: &mut Entries, limit: Option<u64>) -> io::Result<()> {
        while let Some(field) = try!(self.read_entry()) {
            match field.data {
                MultipartData::File(mut file) => {
                    let file = if let Some(limit) = limit {
                        try!(file.save_in_limited(&entries.dir, limit))
                    } else {
                        try!(file.save_in(&entries.dir))
                    };

                    entries.files.insert(field.name, file);
                },
                MultipartData::Text(text) => {
                    entries.fields.insert(field.name, text.into());
                },
            }
        }

        Ok(())
    } 

    fn read_to_string(&mut self) -> io::Result<&str> {
        self.line_buf.clear();

        match self.source.read_to_string(&mut self.line_buf) {
            Ok(read) => Ok(&self.line_buf[..read]),
            Err(err) => Err(err),
        }
    }

    // Consume the next boundary.
    // Returns `true` if the last boundary was read, `false` otherwise.
    fn consume_boundary(&mut self) -> io::Result<bool> {
        debug!("Consume boundary!");
        self.source.consume_boundary()
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
#[derive(Debug)]
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

#[cfg(feature = "nightly")]
fn prepend_str(prefix: &str, mut string: String) -> String {
    string.insert_str(0, prefix);
    string
}

#[cfg(not(feature = "nightly"))]
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


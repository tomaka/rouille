// Copyright 2016 `multipart` Crate Developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

//! `multipart` field header parsing.

use super::httparse::{self, EMPTY_HEADER, Status};

use super::Multipart;
use self::ReadEntryResult::*;

use mime::{Attr, TopLevel, Mime, Value};

use std::io::{self, Read, BufRead, Write};
use std::fs;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::{mem, str};

const RANDOM_FILENAME_LEN: usize = 12;

macro_rules! try_io(
    ($try:expr) => (
        {
            use std::io::{Error, ErrorKind};
            match $try {
                Ok(val) => val,
                Err(e) => return Err(Error::new(ErrorKind::InvalidData, e)),
            }
        }
    )
);

const EMPTY_STR_HEADER: StrHeader<'static> = StrHeader {
    name: "",
    val: "",
};

/// Not exposed
#[derive(Copy, Clone, Debug)]
pub struct StrHeader<'a> {
    name: &'a str,
    val: &'a str,
}


fn with_headers<R, F, Ret>(r: &mut R, f: F) -> io::Result<Ret>
where R: BufRead, F: FnOnce(&[StrHeader]) -> Ret {
    const HEADER_LEN: usize = 4;

    // These are only written once so they don't need to be `mut` or initialized.
    let consume;
    let header_len;

    let mut headers = [EMPTY_STR_HEADER; HEADER_LEN];

    {
        let mut raw_headers = [EMPTY_HEADER; HEADER_LEN];

        loop {
            let buf = try!(r.fill_buf());

            match try_io!(httparse::parse_headers(buf, &mut raw_headers)) {
                Status::Complete((consume_, raw_headers)) =>  {
                    consume = consume_;
                    header_len = raw_headers.len();
                    break;
                },
                Status::Partial => (),
            }
        }

        for (raw, header) in raw_headers.iter().take(header_len).zip(&mut headers) {
            header.name = raw.name;
            header.val = try!(io_str_utf8(raw.value));
        }
    }

    r.consume(consume);

    let headers = &headers[..header_len];

    debug!("Parsed headers: {:?}", headers);

    Ok(f(headers))
}

/// The headers that (may) appear before a `multipart/form-data` field.
pub struct FieldHeaders {
    /// The `Content-Disposition` header, required.
    cont_disp: ContentDisp,
    /// The `Content-Type` header, optional.
    cont_type: Option<Mime>,
}

impl FieldHeaders {
    /// Parse the field headers from the passed `BufRead`, consuming the relevant bytes.
    fn read_from<R: BufRead>(r: &mut R) -> io::Result<Option<Self>> {
        with_headers(r, Self::parse)
    }

    fn parse(headers: &[StrHeader]) -> Option<FieldHeaders> {
        let cont_disp = try_opt!(
                ContentDisp::parse(headers),
                debug!("Failed to read Content-Disposition")
            );

        let cont_type = parse_cont_type(headers);

        Some(FieldHeaders {
            cont_disp: cont_disp,
            cont_type: cont_type,
        })
    }
}

/// The `Content-Disposition` header.
pub struct ContentDisp {
    /// The name of the `multipart/form-data` field.
    field_name: String,
    /// The optional filename for this field.
    filename: Option<String>,
}

impl ContentDisp {
    fn parse(headers: &[StrHeader]) -> Option<ContentDisp> {
        if headers.is_empty() {
            return None;
        }

        const CONT_DISP: &'static str = "Content-Disposition";

        let header = try_opt!(
            find_header(headers, CONT_DISP),
            error!("Field headers did not contain Content-Disposition header (required)")
        );

        const NAME: &'static str = "name=";
        const FILENAME: &'static str = "filename=";

        let after_disp_type = {
            let (disp_type, after_disp_type) = try_opt!(
                split_once(header.val, ';'),
                error!("Expected additional data after Content-Disposition type, got {:?}",
                header.val)
            );


            if disp_type.trim() != "form-data" {
                error!("Unexpected Content-Disposition value: {:?}", disp_type);
                return None;
            };

            after_disp_type
        };

        let (field_name, after_field_name) = try_opt!(
            get_str_after(NAME, ';', after_disp_type),
            error!("Expected field name and maybe filename, got {:?}", after_disp_type)
        );

        let field_name = trim_quotes(field_name);

        let filename = get_str_after(FILENAME, ';', after_field_name)
            .map(|(filename, _)| trim_quotes(filename).to_owned());

        Some(ContentDisp { field_name: field_name.to_owned(), filename: filename })
    }
}

fn parse_cont_type(headers: &[StrHeader]) -> Option<Mime> {
    const CONTENT_TYPE: &'static str = "Content-Type";

    let header = try_opt!(
    find_header(headers, CONTENT_TYPE),
    debug!("Content-Type header not found for field.")
    );

    // Boundary parameter will be parsed into the `Mime`
    debug!("Found Content-Type: {:?}", header.val);
    let content_type = read_content_type(header.val.trim());
    Some(content_type)
}

/// A field in a multipart request. May be either text or a binary stream (file).
#[derive(Debug)]
pub struct MultipartField<M: ReadEntry> {
    /// The field's name from the form
    pub name: String,
    /// The data of the field. Can be text or binary.
    pub data: MultipartData<M>,
}

impl<M: ReadEntry> MultipartField<M> {
    /// Read the next entry in the request.
    pub fn next_entry(self) -> ReadEntryResult<M> {
        self.data.into_inner().read_entry()
    }

    /// Update `self` as the next entry.
    ///
    /// Returns `Ok(Some(self))` if another entry was read, `Ok(None)` if the end of the body was
    /// reached, and `Err(e)` for any errors that occur.
    pub fn next_entry_inplace(&mut self) -> io::Result<Option<&mut Self>> where for<'a> &'a mut M: ReadEntry {
        let multipart = self.data.take_inner();

        match multipart.read_entry() {
            Entry(entry) => {
                *self = entry;
                Ok(Some(self))
            },
            End(multipart) => {
                self.data.give_inner(multipart);
                Ok(None)
            },
            Error(multipart, err) => {
                self.data.give_inner(multipart);
                Err(err)
            }
        }
    }
}

/// The data of a field in a `multipart/form-data` request.
#[derive(Debug)]
pub enum MultipartData<M: ReadEntry> {
    /// The field's payload is a text string.
    Text(MultipartText<M>),
    /// The field's payload is a binary stream (file).
    File(MultipartFile<M>),
    /// The field's payload is a nested multipart body (multiple files).
    Nested(NestedMultipart<M>),
}

impl<M: ReadEntry> MultipartData<M> {
    /// Borrow this payload as a text field, if possible.
    pub fn as_text(&self) -> Option<&str> {
        match *self {
            MultipartData::Text(ref text) => Some(&text.text),
            _ => None,
        }
    }

    /// Borrow this payload as a file field, if possible.
    /// Mutably borrows so the contents can be read.
    pub fn as_file(&mut self) -> Option<&mut MultipartFile<M>> {
        match *self {
            MultipartData::File(ref mut file) => Some(file),
            _ => None,
        }
    }

    /// Return the inner `Multipart`.
    pub fn into_inner(self) -> M {
        use self::MultipartData::*;

        match self {
            Text(text) => text.into_inner(),
            File(file) => file.into_inner(),
            Nested(nested) => nested.into_inner(),
        }
    }

    fn take_inner(&mut self) -> M {
        use self::MultipartData::*;

        match *self {
            Text(ref mut text) => text.take_inner(),
            File(ref mut file) => file.take_inner(),
            Nested(ref mut nested) => nested.take_inner(),
        }
    }

    fn give_inner(&mut self, inner: M) {
        use self::MultipartData::*;

        let inner = Some(inner);

        match *self {
            Text(ref mut text) => text.inner = inner,
            File(ref mut file) => file.inner = inner,
            Nested(ref mut nested) => nested.inner = inner,
        }
    }
}

/// A representation of a text field in a `multipart/form-data` body.
#[derive(Debug)]
pub struct MultipartText<M> {
    /// The text of this field.
    pub text: String,
    /// The `Multipart` this field was read from.
    inner: Option<M>,
}

impl<M> Deref for MultipartText<M> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        &self.text
    }
}

impl<M> Into<String> for MultipartText<M> {
    fn into(self) -> String {
        self.text
    }
}

impl<M> MultipartText<M> {
    fn inner_mut(&mut self) -> &mut M {
        self.inner.as_mut().expect("MultipartText::inner taken!")
    }

    fn take_inner(&mut self) -> M {
        self.inner.take().expect("MultipartText::inner taken!")
    }

    fn into_inner(self) -> M {
        self.inner.expect("MultipartText::inner taken!")
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
pub struct MultipartFile<M> {
    filename: Option<String>,
    content_type: Mime,
    /// The `Multipart` this field was read from.
    inner: Option<M>,
}

impl<M> MultipartFile<M> {
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

    fn inner_mut(&mut self) -> &mut M {
        self.inner.as_mut().expect("MultipartFile::inner taken!")
    }

    fn take_inner(&mut self) -> M {
        self.inner.take().expect("MultipartFile::inner taken!")
    }

    fn into_inner(self) -> M {
        self.inner.expect("MultipartFile::inner taken!")
    }
}

impl<M> MultipartFile<M> where MultipartFile<M>: Read {
    /// Save this file to the given output stream.
    ///
    /// If successful, returns the number of bytes written.
    ///
    /// Retries when `io::Error::kind() == io::ErrorKind::Interrupted`.
    pub fn save_to<W: Write>(&mut self, mut out: W) -> io::Result<u64> {
        retry_on_interrupt(|| io::copy(self, &mut out))
    }

    /// Save this file to the given output stream, **truncated** to `limit`
    /// (no more than `limit` bytes will be written out).
    ///
    /// If successful, returns the number of bytes written.
    ///
    /// Retries when `io::Error::kind() == io::ErrorKind::Interrupted`.
    pub fn save_to_limited<W: Write>(&mut self, mut out: W, limit: u64) -> io::Result<u64> {
        retry_on_interrupt(|| io::copy(&mut self.by_ref().take(limit), &mut out))
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
}

impl<R: Read> Read for MultipartFile<Multipart<R>> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>{
        self.inner_mut().reader.read(buf)
    }
}

impl<R: Read> BufRead for MultipartFile<Multipart<R>> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.inner_mut().reader.fill_buf()
    }

    fn consume(&mut self, amt: usize) {
        self.inner_mut().reader.consume(amt)
    }
}

impl<'a, R: Read + 'a> Read for MultipartFile<&'a mut Multipart<R>> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>{
        self.inner_mut().reader.read(buf)
    }
}

impl<'a, R: Read + 'a> BufRead for MultipartFile<&'a mut Multipart<R>> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.inner_mut().reader.fill_buf()
    }

    fn consume(&mut self, amt: usize) {
        self.inner_mut().reader.consume(amt)
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

fn read_content_type(cont_type: &str) -> Mime {
    cont_type.parse().ok().unwrap_or_else(::mime_guess::octet_stream)
}

fn split_once(s: &str, delim: char) -> Option<(&str, &str)> {
    s.find(delim).map(|idx| s.split_at(idx))
}

fn trim_quotes(s: &str) -> &str {
    s.trim_matches('"')
}

/// Get the string after `needle` in `haystack`, stopping before `end_val_delim`
fn get_str_after<'a>(needle: &str, end_val_delim: char, haystack: &'a str) -> Option<(&'a str, &'a str)> {
    let val_start_idx = try_opt!(haystack.find(needle)) + needle.len();
    let val_end_idx = haystack[val_start_idx..].find(end_val_delim)
        .map_or(haystack.len(), |end_idx| end_idx + val_start_idx);
    Some((&haystack[val_start_idx..val_end_idx], &haystack[val_end_idx..]))
}

fn io_str_utf8(buf: &[u8]) -> io::Result<&str> {
    str::from_utf8(buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

fn find_header<'a, 'b>(headers: &'a [StrHeader<'b>], name: &str) -> Option<&'a StrHeader<'b>> {
    headers.iter().find(|header| header.name == name)
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

fn create_full_path(path: &Path) -> io::Result<fs::File> {
    if let Some(parent) = path.parent() {
        try!(fs::create_dir_all(parent));
    } else {
        // RFC: return an error instead?
        warn!("Attempting to save file in what looks like a root directory. File path: {:?}", path);
    }

    fs::File::create(&path)
}

pub struct NestedEntry<M: ReadEntry> {
    pub content_type: Mime,
    pub filename: Option<String>,
    inner: M
}

#[derive(Debug)]
pub struct NestedMultipart<M: ReadEntry> {
    outer_boundary: Vec<u8>,
    inner: Option<M>,
}

impl<M: ReadEntry> NestedMultipart<M> {
    pub fn read_entry(&mut self) -> ReadEntryResult<&mut M, NestedEntry<&mut M>>
        where for<'a> &'a mut M: ReadEntry {

        let inner = self.inner_mut();

        let headers = match inner.read_headers() {
            Ok(Some(headers)) => headers,
            Ok(None) => return End(inner),
            Err(e) => return Error(inner, e),
        };

        let cont_type = match headers.cont_type {
            Some(cont_type) => cont_type,
            None => return ReadEntryResult::invalid_data(inner,
                                         "Nested multipart requires Content-Type".to_string())
        };

        Entry (
            NestedEntry {
                filename: headers.cont_disp.filename,
                content_type: cont_type,
                inner: inner,
            }
        )
    }

    fn inner_mut(&mut self) -> &mut M {
        self.inner.as_mut().expect("NestedMultipart::inner taken!")
    }

    fn take_inner(&mut self) -> M {
        self.inner.take().expect("NestedMultipart::inner taken!()")
    }

    fn into_inner(mut self) -> M {
        self.restore_boundary();
        self.inner.take().expect("NestedMultipart::inner taken!()")
    }

    fn restore_boundary(&mut self) {
        let multipart = self.inner.as_mut().expect("NestedMultipart::inner taken!()");
        let outer_boundary = mem::replace(&mut self.outer_boundary, Vec::new());
        multipart.swap_boundary(outer_boundary);
    }
}

impl<M: ReadEntry> Drop for NestedMultipart<M> {
    fn drop(&mut self) {
        if self.inner.is_some() {
            self.restore_boundary();
        }
    }
}

/// Public trait but not re-exported.
pub trait ReadEntry: Sized {
    fn read_headers(&mut self) -> io::Result<Option<FieldHeaders>>;

    fn read_to_string(&mut self) -> io::Result<String>;

    fn swap_boundary<B: Into<Vec<u8>>>(&mut self, boundary: B) -> Vec<u8>;

    fn read_entry(mut self) -> ReadEntryResult<Self> {
        let field_headers = match try_read_entry!(self; self.read_headers()) {
            Some(headers) => headers,
            None => return End(self),
        };

        let data = match field_headers.cont_type {
            Some(cont_type) => {
                match cont_type.0 {
                    TopLevel::Multipart if cont_type.1 == "mixed" => {
                        let outer_boundary = match cont_type.get_param(Attr::Boundary) {
                            Some(&Value::Ext(ref boundary)) => self.swap_boundary(&**boundary),
                            _ => {
                                let msg = format!("Nested multipart boundary was not provided for \
                                                   field {:?}", field_headers.cont_disp.field_name);
                                return ReadEntryResult::invalid_data(self, msg);
                            },
                        };

                        MultipartData::Nested(
                            NestedMultipart {
                                outer_boundary: outer_boundary,
                                inner: Some(self),
                            }
                        )
                    },
                    _ => {
                        MultipartData::File(
                            MultipartFile {
                                filename: field_headers.cont_disp.filename,
                                content_type: cont_type,
                                inner: Some(self)
                            }
                        )
                    }
                }
            },
            None => {
                let text = try_read_entry!(self; self.read_to_string());
                MultipartData::Text(MultipartText {
                    text: text,
                    inner: Some(self),
                })
            },
        };

        Entry(
            MultipartField {
                name: field_headers.cont_disp.field_name,
                data: data,
            }
        )
    }
}

impl<R: Read> ReadEntry for Multipart<R> {
    fn read_headers(&mut self) -> io::Result<Option<FieldHeaders>> {
        FieldHeaders::read_from(&mut self.reader)
    }

    fn read_to_string(&mut self) -> io::Result<String> {
        self.read_to_string()
    }

    fn swap_boundary<B: Into<Vec<u8>>>(&mut self, boundary: B) -> Vec<u8> {
        self.reader.swap_boundary(boundary)
    }
}

impl<'a, M: ReadEntry> ReadEntry for &'a mut M {
    fn read_headers(&mut self) -> io::Result<Option<FieldHeaders>> {
        (**self).read_headers()
    }

    fn read_to_string(&mut self) -> io::Result<String> {
        (**self).read_to_string()
    }

    fn swap_boundary<B: Into<Vec<u8>>>(&mut self, boundary: B) -> Vec<u8> {
        (**self).swap_boundary(boundary)
    }
}

/// Result type returned by `Multipart::into_entry()` and `MultipartField::next_entry()`.
pub enum ReadEntryResult<M: ReadEntry, Entry = MultipartField<M>> {
    /// The next entry was found.
    Entry(Entry),
    /// No  more entries could be read.
    End(M),
    /// An error occurred.
    Error(M, io::Error),
}

impl<M: ReadEntry, Entry> ReadEntryResult<M, Entry> {
    pub fn into_result(self) -> io::Result<Option<Entry>> {
        match self {
            ReadEntryResult::Entry(entry) => Ok(Some(entry)),
            ReadEntryResult::End(_) => Ok(None),
            ReadEntryResult::Error(_, err) => Err(err),
        }
    }

    fn invalid_data(multipart: M, msg: String) -> Self {
        ReadEntryResult::Error (
            multipart,
            io::Error::new(io::ErrorKind::InvalidData, msg),
        )
    }
}
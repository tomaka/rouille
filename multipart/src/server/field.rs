// Copyright 2016 `multipart` Crate Developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

//! `multipart` field header parsing.

use super::httparse::{self, EMPTY_HEADER, Status};

use super::{Multipart, ReadEntryResult};

use mime::{Attr, Mime, Value};

use std::borrow::BorrowMut;
use std::io::{self, Read, BufRead, Write};
use std::fs::{self, File};
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

#[derive(Copy, Clone, Debug)]
struct StrHeader<'a> {
    name: &'a str,
    val: &'a str,
}

/// The headers that (may) appear before a `multipart/form-data` field.
struct FieldHeaders {
    /// The `Content-Disposition` header, required.
    cont_disp: ContentDisp,
    /// The `Content-Type` header, optional.
    cont_type: Option<ContentType>,
}

impl FieldHeaders {
    /// Parse the field headers from the passed `BufRead`, consuming the relevant bytes.
    fn read_from<R: BufRead>(r: &mut R) -> io::Result<Option<Self>> {
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

        let headers = &headers[..header_len];

        debug!("Parsed field headers: {:?}", headers);

        r.consume(consume);

        Ok(Self::parse(headers))
    }

    fn parse(headers: &[StrHeader]) -> Option<FieldHeaders> {
        let cont_disp = try_opt!(
                ContentDisp::parse(headers),
                debug!("Failed to read Content-Disposition")
            );

        let cont_type = ContentType::parse(headers);

        Some(FieldHeaders {
            cont_disp: cont_disp,
            cont_type: cont_type,
        })
    }
}

/// The `Content-Disposition` header.
struct ContentDisp {
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

/// The `Content-Type` header.
struct ContentType {
    /// The MIME type of the `multipart` field.
    ///
    /// May contain a sub-boundary parameter.
    val: Mime,
}

impl ContentType {
    fn parse(headers: &[StrHeader]) -> Option<ContentType> {
        const CONTENT_TYPE: &'static str = "Content-Type";

        let header = try_opt!(
            find_header(headers, CONTENT_TYPE),
            debug!("Content-Type header not found for field.")
        );

        // Boundary parameter will be parsed into the `Mime`
        debug!("Found Content-Type: {:?}", header.val);
        let content_type = read_content_type(header.val.trim());
        Some(ContentType { val: content_type })
    }

    /// Get the optional boundary parameter for this `Content-Type`.
    #[allow(dead_code)]
    pub fn boundary(&self) -> Option<&str> {
        self.val.get_param(Attr::Boundary).map(Value::as_str)
    }
}

/// A field in a multipart request. May be either text or a binary stream (file).
#[derive(Debug)]
pub struct MultipartField<M> {
    /// The field's name from the form
    pub name: String,
    /// The data of the field. Can be text or binary.
    pub data: MultipartData<M>,
}

impl<M: ReadField> MultipartField<M> {
    /// Read the next entry in the request.
    pub fn next_entry(self) -> ReadEntryResult<M> {
        self.data.into_inner().read_field()
    }

    /// Update `self` as the next entry.
    ///
    /// Returns `Ok(Some(self))` if another entry was read, `Ok(None)` if the end of the body was
    /// reached, and `Err(e)` for any errors that occur.
    pub fn next_entry_inplace(&mut self) -> io::Result<Option<&mut Self>> where for<'a> &'a mut M: ReadField {
        let entry = match try!(self.read_entry()) {
            Some(pair) => pair,
            None => return Ok(None),
        };

        let multipart = mem::replace(&mut self.data, MultipartData::_Swapping).into_inner();

        *self = entry.set_inner(multipart);

        Ok(Some(self))
    }

    fn read_entry(&mut self) -> io::Result<Option<MultipartField<()>>> where for<'a> &'a mut M: ReadField {
        self.data.inner_mut().read_field()
            .map_err(|e| e.error)
            .map(|entry| entry.map(|entry| entry.set_inner(())))
    }
}

impl<M> MultipartField<M> {
    fn set_inner<M_>(self, new_inner: M_) -> MultipartField<M_> {
        MultipartField {
            name: self.name,
            data: self.data.set_inner(new_inner).1
        }
    }
}

pub fn read_field<R: Read, M: BorrowMut<Multipart<R>>>(mut multipart: M) -> ReadEntryResult<M> {
    let field_headers = match try_read_entry!(multipart; FieldHeaders::read_from(&mut multipart.borrow_mut().reader)) {
        Some(headers) => headers,
        None => return Ok(None),
    };

    let data = match field_headers.cont_type {
        Some(content_type) => {
            MultipartData::File(
                MultipartFile::from_stream(
                    field_headers.cont_disp.filename,
                    content_type.val,
                    multipart,
                )
            )
        },
        None => {
            let text = try_read_entry!(multipart; multipart.borrow_mut().read_to_string());
            MultipartData::Text(MultipartText {
                text: text,
                multipart: multipart,
            })
        },
    };

    Ok(Some(
        MultipartField {
            name: field_headers.cont_disp.field_name,
            data: data,
        }
    ))
}

/// The data of a field in a `multipart/form-data` request.
#[derive(Debug)]
pub enum MultipartData<M> {
    /// The field's payload is a text string.
    Text(MultipartText<M>),
    /// The field's payload is a binary stream (file).
    File(MultipartFile<M>),
    // TODO: Support multiple files per field (nested boundaries)
    // MultiFiles(Vec<MultipartFile>),
    #[doc(hidden)]
    _Swapping,
}

impl<M> MultipartData<M> {
    /// Return the inner `Multipart`.
    pub fn into_inner(self) -> M {
        use self::MultipartData::*;

        match self {
            Text(text) => text.multipart,
            File(file) => file.multipart,
            _Swapping => unreachable!("MultipartData::_Swapping was left in-place somehow"),
        }
    }

    fn inner_mut(&mut self) -> &mut M {
        use self::MultipartData::*;

        match *self {
            Text(ref mut text) => &mut text.multipart,
            File(ref mut file) => &mut file.multipart,
            _Swapping => unreachable!("MultipartData::_Swapping was left in-place somehow"),
        }
    }

    fn set_inner<M_>(self, new_inner: M_) -> (M, MultipartData<M_>) {
        use self::MultipartData::*;

        match self {
            Text(text) => (text.multipart, Text(MultipartText { text: text.text, multipart: new_inner })),
            File(file) => (file.multipart, File(MultipartFile {
                filename: file.filename,
                content_type: file.content_type,
                multipart: new_inner
            })),
            _Swapping => unreachable!("MultipartData::_Swapping was left in-place somehow"),
        }
    }

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
}

/// A representation of a text field in a `multipart/form-data` body.
#[derive(Debug)]
pub struct MultipartText<M> {
    /// The text of this field.
    pub text: String,
    /// The `Multipart` this field was read from.
    pub multipart: M,
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
    pub multipart: M,
}

impl<M> MultipartFile<M> {
    fn from_stream(filename: Option<String>,
                   content_type: Mime,
                   multipart: M) -> MultipartFile<M> {
        MultipartFile {
            filename: filename,
            content_type: content_type,
            multipart: multipart,
        }
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
        self.multipart.reader.read(buf)
    }
}

impl<R: Read> BufRead for MultipartFile<Multipart<R>> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.multipart.reader.fill_buf()
    }

    fn consume(&mut self, amt: usize) {
        self.multipart.reader.consume(amt)
    }
}

impl<'a, R: Read + 'a> Read for MultipartFile<&'a mut Multipart<R>> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>{
        self.multipart.reader.read(buf)
    }
}

impl<'a, R: Read + 'a> BufRead for MultipartFile<&'a mut Multipart<R>> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.multipart.reader.fill_buf()
    }

    fn consume(&mut self, amt: usize) {
        self.multipart.borrow_mut().reader.consume(amt)
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

fn create_full_path(path: &Path) -> io::Result<File> {
    if let Some(parent) = path.parent() {
        try!(fs::create_dir_all(parent));
    } else {
        // RFC: return an error instead?
        warn!("Attempting to save file in what looks like a root directory. File path: {:?}", path);
    }

    File::create(&path)
}

pub trait ReadField: Sized {
    fn read_field(self) -> ReadEntryResult<Self>;
}

impl<R: Read> ReadField for Multipart<R> {
    fn read_field(self) -> ReadEntryResult<Self> {
        read_field(self)
    }
}

impl<'a, R: Read + 'a> ReadField for &'a mut Multipart<R> {
    fn read_field(self) -> ReadEntryResult<Self> {
        read_field(self)
    }
}

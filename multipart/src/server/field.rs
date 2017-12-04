// Copyright 2016 `multipart` Crate Developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

//! `multipart` field header parsing.

use super::httparse::{self, EMPTY_HEADER, Header, Status};

use self::ReadEntryResult::*;

use super::save::{SaveBuilder, SavedFile};

use mime::{TopLevel, Mime};

use std::io::{self, Read, BufRead, Write};
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::{str, fmt, error};

use std::ascii::AsciiExt;

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

const MAX_ATTEMPTS: usize = 30;

fn with_headers<R, F, Ret>(r: &mut R, closure: F) -> Result<Ret, ParseHeaderError>
where R: BufRead, F: FnOnce(&[StrHeader]) -> Ret {
    const HEADER_LEN: usize = 4;

    // These are only written once so they don't need to be `mut` or initialized.
    let consume;
    let ret;

    let mut attempts = 0;

    loop {
        let mut raw_headers = [EMPTY_HEADER; HEADER_LEN];

        let buf = r.fill_buf()?;

        if attempts == MAX_ATTEMPTS {
            return Err(ParseHeaderError::Other("Could not read field headers".to_string()));
        }

        match httparse::parse_headers(buf, &mut raw_headers) {
            Ok(Status::Complete((consume_, raw_headers))) =>  {
                consume = consume_;
                let mut headers = [EMPTY_STR_HEADER; HEADER_LEN];
                let headers = copy_headers(raw_headers, &mut headers)?;
                debug!("Parsed headers: {:?}", headers);
                ret = closure(headers);
                break;
            },
            Ok(Status::Partial) => {
                attempts += 1;
                continue;
            },
            Err(err) => return Err(ParseHeaderError::from(err)),
        };
    }

    r.consume(consume);
    Ok(ret)
}

fn copy_headers<'h, 'b: 'h>(raw: &[Header<'b>], headers: &'h mut [StrHeader<'b>]) -> io::Result<&'h [StrHeader<'b>]> {
    for (raw, header) in raw.iter().zip(&mut *headers) {
        header.name = raw.name;
        header.val = io_str_utf8(raw.value)?;
    }

    Ok(&headers[..raw.len()])
}

/// The headers that (may) appear before a `multipart/form-data` field.
///
/// ### Warning: Values are Client-Provided
/// Everything in this struct are values from the client and should be considered **untrustworthy**.
/// This crate makes no effort to validate or sanitize any client inputs.
pub struct FieldHeaders {
    /// The field's name from the form.
    pub name: Arc<str>,

    /// The filename of this entry, if supplied. This is not guaranteed to match the original file
    /// or even to be a valid filename for the current platform.
    pub filename: Option<String>,

    /// The MIME type (`Content-Type` value) of this file, if supplied by the client.
    ///
    /// If this is not supplied, the content-type of the field should default to `text/plain` as
    /// per [IETF RFC 7578, section 4.4](https://tools.ietf.org/html/rfc7578#section-4.4), but this
    /// should not be implicitly trusted. This crate makes no attempt to identify or validate
    /// the content-type of the actual field data.
    pub content_type: Option<Mime>,
}

impl FieldHeaders {
    /// Parse the field headers from the passed `BufRead`, consuming the relevant bytes.
    fn read_from<R: BufRead>(r: &mut R) -> Result<Self, ParseHeaderError> {
        with_headers(r, Self::parse)?
    }

    fn parse(headers: &[StrHeader]) -> Result<FieldHeaders, ParseHeaderError> {
        let cont_disp = ContentDisp::parse(headers)?.ok_or(ParseHeaderError::MissingContentDisposition)?;
        Ok(FieldHeaders {
            name: cont_disp.name,
            filename: cont_disp.filename,
            content_type: parse_content_type(headers)?,
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
    fn parse(headers: &[StrHeader]) -> Result<Option<ContentDisp>, ParseHeaderError> {
        const CONT_DISP: &'static str = "Content-Disposition";
        let header = if let Some(header) = find_header(headers, CONT_DISP) {
            header
        } else {
            return Ok(None);
        };

        const NAME: &'static str = "name=";
        const FILENAME: &'static str = "filename=";

        let after_disp_type = match split_once(header.val, ';') {
            Some((disp_type, after_disp_type)) => {
                if disp_type.trim() != "form-data" {
                    let err = format!("Unexpected Content-Disposition value: {:?}", disp_type);
                    return Err(ParseHeaderError::Invalid(err));
                }
                after_disp_type
            },
            None => {
                let err = format!("Expected additional data after Content-Disposition type, got {:?}", header.val);
                return Err(ParseHeaderError::Invalid(err));
            }
        };

        let (field_name, filename) = match get_str_after(NAME, ';', after_disp_type) {
            None => {
                let err = format!("Expected field name and maybe filename, got {:?}", after_disp_type);
                return Err(ParseHeaderError::Invalid(err));
            },
            Some((field_name, after_field_name)) => {
                let field_name = trim_quotes(field_name);
                let filename = get_str_after(FILENAME, ';', after_field_name).map(|(filename, _)| trim_quotes(filename).to_owned());
                (field_name, filename)
            },
        };

        Ok(Some(ContentDisp { field_name: field_name.to_owned(), filename: filename }))
    }
}

fn parse_content_type(headers: &[StrHeader]) -> Result<Option<Mime>, ParseHeaderError> {
    const CONTENT_TYPE: &'static str = "Content-Type";
    let header = if let Some(header) = find_header(headers, CONTENT_TYPE) {
        header
    } else {
        return Ok(None)
    };

    // Boundary parameter will be parsed into the `Mime`
    debug!("Found Content-Type: {:?}", header.val);
    Ok(Some(read_content_type(header.val.trim())))
}

/// A field in a multipart request with its associated headers and data.
///
/// ### Warning: Values are Client-Provided
/// Everything in this struct are values from the client and should be considered **untrustworthy**.
/// This crate makes no effort to validate or sanitize any client inputs.
#[derive(Debug)]
pub struct MultipartField<M: ReadEntry> {
    /// The headers for this field, including the name, filename, and content-type, if provided.
    ///
    /// ### Warning: Values are Client-Provided
    /// Everything in this struct are values from the client and should be considered **untrustworthy**.
    /// This crate makes no effort to validate or sanitize any client inputs.
    pub headers: FieldHeaders,

    /// The field's data.
    pub data: MultipartData<M>,
}

impl<M: ReadEntry> MultipartField<M> {
    /// Returns `true` if this field has no content-type or the content-type is `text/plain`.
    ///
    /// This typically means it can be read to a string, but it could still be using an unsupported
    /// character encoding, so decoding to `String` needs to ensure that the data is valid UTF-8.
    ///
    /// Note also that the field contents may be too large to reasonably fit in memory.
    /// The `.save()` adapter can be used to enforce a size limit.
    ///
    /// Detecting character encodings by any means is (currently) beyond the scope of this crate.
    pub fn is_text(&self) -> bool {
        self.headers.content_type.as_ref()
            .map(|ct| ct.type_() == mime::TEXT && ct.subtype() == mime::PLAIN)
            .unwrap_or(true)
    }

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
///
/// You can read it to EOF, or use the `save()` adaptor to save it to disk/memory.
#[derive(Debug)]
pub struct MultipartData<M> {
    inner: Option<M>,
}

impl<M> MultipartData<M> where M: ReadEntry {
    /// Get a builder type which can save the file with or without a size limit.
    pub fn save(&mut self) -> SaveBuilder<&mut Self> {
        SaveBuilder::new(self)
    }

    fn inner_mut(&mut self) -> &mut M {
        self.inner.as_mut().expect("MultipartFile::inner taken!")
    }

    fn take_inner(&mut self) -> M {
        self.inner.take().expect("MultipartFile::inner already taken!")
    }

    fn into_inner(self) -> M {
        self.inner.expect("MultipartFile::inner taken!")
    }

    fn give_inner(&mut self, inner: M) {
        self.inner = Some(inner);
    }
}

impl<M: ReadEntry> Read for MultipartData<M> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>{
        self.inner_mut().source().read(buf)
    }
}

impl<M: ReadEntry> BufRead for MultipartData<M> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.inner_mut().source().fill_buf()
    }

    fn consume(&mut self, amt: usize) {
        self.inner_mut().source().consume(amt)
    }
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
    // Field names are case insensitive and consist of ASCII characters
    // only (see https://tools.ietf.org/html/rfc822#section-3.2).
    headers.iter().find(|header| header.name.eq_ignore_ascii_case(name))
}

/// Common trait for `Multipart` and `&mut Multipart`
pub trait ReadEntry: PrivReadEntry + Sized {
    /// Attempt to read the next entry in the multipart stream.
    fn read_entry(mut self) -> ReadEntryResult<Self> {
        debug!("ReadEntry::read_entry()");

        if try_read_entry!(self; self.consume_boundary()) {
            return End(self);
        }

        let field_headers: FieldHeaders = try_read_entry!(self; self.read_headers());

        match field_headers.cont_type {
            Some(ref cont_type) if cont_type.type_() == mime::MULTIPART => {
                let msg = format!("Error on field {:?}: nested multipart fields are \
                                           not supported. However, reports of clients sending \
                                           requests like this are welcome at \
                                           https://github.com/abonander/multipart/issues/56",
                                  field_headers.cont_disp.field_name);

                return ReadEntryResult::invalid_data(self, msg);
            },
            _ => (),
        }

        Entry(
            MultipartField {
                headers: field_headers,
                data: MultipartData {
                    inner: Some(self),
                },
            }
        )
    }

    /// Equivalent to `read_entry()` but takes `&mut self`
    fn read_entry_mut(&mut self) -> ReadEntryResult<&mut Self> {
        ReadEntry::read_entry(self)
    }
}

impl<T> ReadEntry for T where T: PrivReadEntry {}

/// Public trait but not re-exported.
pub trait PrivReadEntry {
    type Source: BufRead;

    fn source(&mut self) -> &mut Self::Source;

    /// Consume the next boundary.
    /// Returns `true` if the last boundary was read, `false` otherwise.
    fn consume_boundary(&mut self) -> io::Result<bool>;

    fn read_headers(&mut self) -> Result<FieldHeaders, io::Error> {
        FieldHeaders::read_from(&mut self.source())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }

    fn read_to_string(&mut self) -> io::Result<String> {
        let mut buf = String::new();

        match self.source().read_to_string(&mut buf) {
            Ok(_) => Ok(buf),
            Err(err) => Err(err),
        }
    }
}

impl<'a, M: ReadEntry> PrivReadEntry for &'a mut M {
    type Source = M::Source;

    fn source(&mut self) -> &mut M::Source {
        (**self).source()
    }

    fn consume_boundary(&mut self) -> io::Result<bool> {
        (**self).consume_boundary()
    }
}

/// Ternary result type returned by `ReadEntry::next_entry()`,
/// `Multipart::into_entry()` and `MultipartField::next_entry()`.
pub enum ReadEntryResult<M: ReadEntry, Entry = MultipartField<M>> {
    /// The next entry was found.
    Entry(Entry),
    /// No  more entries could be read.
    End(M),
    /// An error occurred.
    Error(M, io::Error),
}

impl<M: ReadEntry, Entry> ReadEntryResult<M, Entry> {
    /// Convert `self` into `Result<Option<Entry>>` as follows:
    ///
    /// * `Entry(entry) -> Ok(Some(entry))`
    /// * `End(_) -> Ok(None)`
    /// * `Error(_, err) -> Err(err)`
    pub fn into_result(self) -> io::Result<Option<Entry>> {
        match self {
            ReadEntryResult::Entry(entry) => Ok(Some(entry)),
            ReadEntryResult::End(_) => Ok(None),
            ReadEntryResult::Error(_, err) => Err(err),
        }
    }

    /// Attempt to unwrap `Entry`, panicking if this is `End` or `Error`.
    pub fn unwrap(self) -> Entry {
        self.expect_alt("`ReadEntryResult::unwrap()` called on `End` value",
                        "`ReadEntryResult::unwrap()` called on `Error` value: {:?}")
    }

    /// Attempt to unwrap `Entry`, panicking if this is `End` or `Error`
    /// with the given message. Adds the error's message in the `Error` case.
    pub fn expect(self, msg: &str) -> Entry {
        self.expect_alt(msg, msg)
    }

    /// Attempt to unwrap `Entry`, panicking if this is `End` or `Error`.
    /// If this is `End`, panics with `end_msg`; if `Error`, panics with `err_msg`
    /// as well as the error's message.
    pub fn expect_alt(self, end_msg: &str, err_msg: &str) -> Entry {
        match self {
            Entry(entry) => entry,
            End(_) => panic!("{}", end_msg),
            Error(_, err) => panic!("{}: {:?}", err_msg, err),
        }
    }

    /// Attempt to unwrap as `Option<Entry>`, panicking in the `Error` case.
    pub fn unwrap_opt(self) -> Option<Entry> {
        self.expect_opt("`ReadEntryResult::unwrap_opt()` called on `Error` value")
    }

    /// Attempt to unwrap as `Option<Entry>`, panicking in the `Error` case
    /// with the given message as well as the error's message.
    pub fn expect_opt(self, msg: &str) -> Option<Entry> {
        match self {
            Entry(entry) => Some(entry),
            End(_) => None,
            Error(_, err) => panic!("{}: {:?}", msg, err),
        }
    }

    fn invalid_data(multipart: M, msg: String) -> Self {
        ReadEntryResult::Error (
            multipart,
            io::Error::new(io::ErrorKind::InvalidData, msg),
        )
    }
}


#[derive(Debug)]
enum ParseHeaderError {
    /// The `Content-Disposition` header was not found
    MissingContentDisposition,
    /// The header was found but could not be parsed
    Invalid(String),
    /// IO error
    Io(io::Error),
    Other(String),
}

impl fmt::Display for ParseHeaderError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            ParseHeaderError::MissingContentDisposition => write!(f, "\"Content-Disposition\" header not found (ParseHeaderError::MissingContentDisposition)"),
            ParseHeaderError::Invalid(ref msg) => write!(f, "invalid header (ParseHeaderError::Invalid({}))", msg),
            ParseHeaderError::Io(_) => write!(f, "could not read header (ParseHeaderError::Io)"),
            ParseHeaderError::Other(ref reason) => write!(f, "unknown parsing error (ParseHeaderError::Other(\"{}\"))", reason),
        }
    }
}

impl error::Error for ParseHeaderError {
    fn description(&self) -> &str {
        match *self {
            ParseHeaderError::MissingContentDisposition => "\"Content-Disposition\" header not found",
            ParseHeaderError::Invalid(_) => "the header is not formatted correctly",
            ParseHeaderError::Io(_) => "failed to read the header",
            ParseHeaderError::Other(_) => "unknown parsing error",
        }
    }

    fn cause(&self) -> Option<&error::Error> {
        match *self {
            ParseHeaderError::Io(ref e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for ParseHeaderError {
    fn from(err: io::Error) -> ParseHeaderError {
        ParseHeaderError::Io(err)
    }
}

impl From<httparse::Error> for ParseHeaderError {
    fn from(err: httparse::Error) -> ParseHeaderError {
        ParseHeaderError::Invalid(format!("{}", err))
    }
}

#[test]
fn test_find_header() {
    let headers = [
        StrHeader { name: "Content-Type", val: "text/plain" },
        StrHeader { name: "Content-disposition", val: "form-data" },
        StrHeader { name: "content-transfer-encoding", val: "binary" }
    ];

    assert_eq!(find_header(&headers, "Content-Type").unwrap().val, "text/plain");
    assert_eq!(find_header(&headers, "Content-Disposition").unwrap().val, "form-data");
    assert_eq!(find_header(&headers, "Content-Transfer-Encoding").unwrap().val, "binary");
}

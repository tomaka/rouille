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

        // FIXME: https://github.com/seanmonstar/httparse/issues/34
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
pub struct FieldHeaders {
    /// The `Content-Disposition` header, required.
    cont_disp: ContentDisp,
    /// The `Content-Type` header, optional.
    cont_type: Option<Mime>,
}

impl FieldHeaders {
    /// Parse the field headers from the passed `BufRead`, consuming the relevant bytes.
    fn read_from<R: BufRead>(r: &mut R) -> Result<Self, ParseHeaderError> {
        with_headers(r, Self::parse)?
    }

    fn parse(headers: &[StrHeader]) -> Result<FieldHeaders, ParseHeaderError> {
        let cont_disp = ContentDisp::parse(headers)?.ok_or(ParseHeaderError::MissingContentDisposition)?;
        Ok(FieldHeaders {
            cont_disp: cont_disp,
            cont_type: parse_cont_type(headers)?,
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

fn parse_cont_type(headers: &[StrHeader]) -> Result<Option<Mime>, ParseHeaderError> {
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
pub enum MultipartData<M> {
    /// The field's payload is a text string.
    Text(MultipartText<M>),
    /// The field's payload is a binary stream (file).
    File(MultipartFile<M>),
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
        }
    }

    fn take_inner(&mut self) -> M {
        use self::MultipartData::*;

        match *self {
            Text(ref mut text) => text.take_inner(),
            File(ref mut file) => file.take_inner(),
        }
    }

    fn give_inner(&mut self, inner: M) {
        use self::MultipartData::*;

        let inner = Some(inner);

        match *self {
            Text(ref mut text) => text.inner = inner,
            File(ref mut file) => file.inner = inner,
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
    #[doc(hidden)]
    pub fn take_inner(&mut self) -> M {
        self.inner.take().expect("MultipartText::inner already taken!")
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
/// You can read it to EOF, or use one of the `save()` method
/// to save it to disk.
#[derive(Debug)]
pub struct MultipartFile<M> {
    /// The filename of this entry, if supplied.
    ///
    /// ### Warning: Client Provided / Untrustworthy
    /// You should treat this value as **untrustworthy** because it is an arbitrary string
    /// provided by the client.
    ///
    /// It is a serious security risk to create files or directories with paths based on user input.
    /// A malicious user could craft a path which can be used to overwrite important files, such as
    /// web templates, static assets, Javascript files, database files, configuration files, etc.,
    /// if they are writable by the server process.
    ///
    /// This can be mitigated somewhat by setting filesystem permissions as
    /// conservatively as possible and running the server under its own user with restricted
    /// permissions, but you should still not use user input directly as filesystem paths.
    /// If it is truly necessary, you should sanitize filenames such that they cannot be
    /// misinterpreted by the OS. Such functionality is outside the scope of this crate.
    pub filename: Option<String>,

    /// The MIME type (`Content-Type` value) of this file, if supplied by the client,
    /// or `"applicaton/octet-stream"` otherwise.
    ///
    /// ### Note: Client Provided
    /// Consider this value to be potentially untrustworthy, as it is provided by the client.
    /// It may be inaccurate or entirely wrong, depending on how the client determined it.
    ///
    /// Some variants wrap arbitrary strings which could be abused by a malicious user if your
    /// application performs any non-idempotent operations based on their value, such as
    /// starting another program or querying/updating a database (web-search "SQL injection").
    pub content_type: Mime,

    /// The `Multipart` this field was read from.
    inner: Option<M>,
}

impl<M> MultipartFile<M> {
    /// Get the filename of this entry, if supplied.
    ///
    /// ### Warning: Client Provided / Untrustworthy
    /// You should treat this value as **untrustworthy** because it is an arbitrary string
    /// provided by the client.
    ///
    /// It is a serious security risk to create files or directories with paths based on user input.
    /// A malicious user could craft a path which can be used to overwrite important files, such as
    /// web templates, static assets, Javascript files, database files, configuration files, etc.,
    /// if they are writable by the server process.
    ///
    /// This can be mitigated somewhat by setting filesystem permissions as
    /// conservatively as possible and running the server under its own user with restricted
    /// permissions, but you should still not use user input directly as filesystem paths.
    /// If it is truly necessary, you should sanitize filenames such that they cannot be
    /// misinterpreted by the OS. Such functionality is outside the scope of this crate.
    #[deprecated(since = "0.10.0", note = "`filename` field is now public")]
    pub fn filename(&self) -> Option<&str> {
        self.filename.as_ref().map(String::as_ref)
    }

    /// Get the MIME type (`Content-Type` value) of this file, if supplied by the client,
    /// or `"applicaton/octet-stream"` otherwise.
    ///
    /// ### Note: Client Provided
    /// Consider this value to be potentially untrustworthy, as it is provided by the client.
    /// It may be inaccurate or entirely wrong, depending on how the client determined it.
    ///
    /// Some variants wrap arbitrary strings which could be abused by a malicious user if your
    /// application performs any non-idempotent operations based on their value, such as
    /// starting another program or querying/updating a database (web-search "SQL injection").
    #[deprecated(since = "0.10.0", note = "`content_type` field is now public")]
    pub fn content_type(&self) -> &Mime {
        &self.content_type
    }


    fn inner_mut(&mut self) -> &mut M {
        self.inner.as_mut().expect("MultipartFile::inner taken!")
    }

    #[doc(hidden)]
    pub fn take_inner(&mut self) -> M {
        self.inner.take().expect("MultipartFile::inner already taken!")
    }

    fn into_inner(self) -> M {
        self.inner.expect("MultipartFile::inner taken!")
    }
}

impl<M> MultipartFile<M> where M: ReadEntry {
    /// Get a builder type which can save the file with or without a size limit.
    pub fn save(&mut self) -> SaveBuilder<&mut Self> {
        SaveBuilder::new(self)
    }

    /// Save this file to the given output stream.
    ///
    /// If successful, returns the number of bytes written.
    ///
    /// Retries when `io::Error::kind() == io::ErrorKind::Interrupted`.
    #[deprecated(since = "0.10.0", note = "use `.save().write_to()` instead")]
    pub fn save_to<W: Write>(&mut self, out: W) -> io::Result<u64> {
        self.save().write_to(out).into_result_strict()
    }

    /// Save this file to the given output stream, **truncated** to `limit`
    /// (no more than `limit` bytes will be written out).
    ///
    /// If successful, returns the number of bytes written.
    ///
    /// Retries when `io::Error::kind() == io::ErrorKind::Interrupted`.
    #[deprecated(since = "0.10.0", note = "use `.save().size_limit(limit).write_to(out)` instead")]
    pub fn save_to_limited<W: Write>(&mut self, out: W, limit: u64) -> io::Result<u64> {
        self.save().size_limit(limit).write_to(out).into_result_strict()
    }

    /// Save this file to `path`.
    ///
    /// Returns the saved file info on success, or any errors otherwise.
    ///
    /// Retries when `io::Error::kind() == io::ErrorKind::Interrupted`.
    #[deprecated(since = "0.10.0", note = "use `.save().with_path(path)` instead")]
    pub fn save_as<P: Into<PathBuf>>(&mut self, path: P) -> io::Result<SavedFile> {
        self.save().with_path(path).into_result_strict()
    }

    /// Save this file in the directory pointed at by `dir`,
    /// using a random alphanumeric string as the filename.
    ///
    /// Any missing directories in the `dir` path will be created.
    ///
    /// Returns the saved file's info on success, or any errors otherwise.
    ///
    /// Retries when `io::Error::kind() == io::ErrorKind::Interrupted`.
    #[deprecated(since = "0.10.0", note = "use `.save().with_dir(dir)` instead")]
    pub fn save_in<P: AsRef<Path>>(&mut self, dir: P) -> io::Result<SavedFile> {
        self.save().with_dir(dir.as_ref()).into_result_strict()
    }

    /// Save this file to `path`, **truncated** to `limit` (no more than `limit` bytes will be written out).
    ///
    /// Any missing directories in the `dir` path will be created.
    ///
    /// Returns the saved file's info on success, or any errors otherwise.
    ///
    /// Retries when `io::Error::kind() == io::ErrorKind::Interrupted`.
    #[deprecated(since = "0.10.0", note = "use `.save().size_limit(limit).with_path(path)` instead")]
    pub fn save_as_limited<P: Into<PathBuf>>(&mut self, path: P, limit: u64) -> io::Result<SavedFile> {
        self.save().size_limit(limit).with_path(path).into_result_strict()
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
    #[deprecated(since = "0.10.0", note = "use `.save().size_limit(limit).with_dir(dir)` instead")]
    pub fn save_in_limited<P: AsRef<Path>>(&mut self, dir: P, limit: u64) -> io::Result<SavedFile> {
        self.save().size_limit(limit).with_dir(dir).into_result_strict()
    }
}

impl<M: ReadEntry> Read for MultipartFile<M> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize>{
        self.inner_mut().source().read(buf)
    }
}

impl<M: ReadEntry> BufRead for MultipartFile<M> {
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
    /// Field names are case insensitive and consist of ASCII characters
    /// only (see https://tools.ietf.org/html/rfc822#section-3.2).
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

        let field_headers = try_read_entry!(self; self.read_headers());

        let data = match field_headers.cont_type {
            Some(cont_type) => {
                match cont_type.0 {
                    TopLevel::Multipart => {
                        let msg = format!("Error on field {:?}: nested multipart fields are \
                                           not supported. However, reports of clients sending \
                                           requests like this are welcome at \
                                           https://github.com/abonander/multipart/issues/56",
                                          field_headers.cont_disp.field_name);

                        return ReadEntryResult::invalid_data(self, msg);
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

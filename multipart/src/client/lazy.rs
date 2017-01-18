//! Multipart requests which write out their data in one fell swoop.

use log::LogLevel;

use mime::Mime;

use std::borrow::Cow;
use std::error::Error;
use std::fs::File;
use std::path::{Path, PathBuf};

use std::io::prelude::*;
use std::io::Cursor;
use std::{fmt, io, mem, vec};

use super::{HttpRequest, HttpStream, MultipartWriter};

macro_rules! try_lazy (
    ($field:expr, $try:expr) => (
        match $try {
            Ok(ok) => ok,
            Err(e) => return Err(LazyError::with_field($field, e)),
        }
    );
    ($try:expr) => (
        match $try {
            Ok(ok) => ok,
            Err(e) => return Err(LazyError::without_field(e)),
        }
    )
);

/// A `LazyError` wrapping `std::io::Error`.
pub type LazyIoError<'a> = LazyError<'a, io::Error>;

/// `Result` type for `LazyIoError`.
pub type LazyIoResult<'a, T> = Result<T, LazyIoError<'a>>;

/// An error for lazily written multipart requests, including the original error as well
/// as the field which caused the error, if applicable.
pub struct LazyError<'a, E> {
    /// The field that caused the error.
    /// If `None`, there was a problem opening the stream to write or finalizing the stream.
    pub field_name: Option<Cow<'a, str>>,
    /// The inner error.
    pub error: E,
    /// Private field for back-compat.
    _priv: (),
}

impl<'a, E> LazyError<'a, E> {
    fn without_field<E_: Into<E>>(error: E_) -> Self {
        LazyError {
            field_name: None,
            error: error.into(),
            _priv: (),
        }
    }

    fn with_field<E_: Into<E>>(field_name: Cow<'a, str>, error: E_) -> Self {
        LazyError {
            field_name: Some(field_name),
            error: error.into(),
            _priv: (),
        }
    }
}

/// Take `self.error`, discarding `self.field_name`.
impl<'a> Into<io::Error> for LazyError<'a, io::Error> {
    fn into(self) -> io::Error {
        self.error
    }
}

impl<'a, E: Error> Error for LazyError<'a, E> {
    fn description(&self) -> &str {
        self.error.description()
    }

    fn cause(&self) -> Option<&Error> {
        Some(&self.error)
    }
}

impl<'a, E: fmt::Debug> fmt::Debug for LazyError<'a, E> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        if let Some(ref field_name) = self.field_name {
            fmt.write_fmt(format_args!("LazyError (on field {:?}): {:?}", field_name, self.error))
        } else {
            fmt.write_fmt(format_args!("LazyError (misc): {:?}", self.error))
        }
    }
}

impl<'a, E: fmt::Display> fmt::Display for LazyError<'a, E> {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        if let Some(ref field_name) = self.field_name {
            fmt.write_fmt(format_args!("Error writing field {:?}: {}", field_name, self.error))
        } else {
            fmt.write_fmt(format_args!("Error opening or flushing stream: {}", self.error))
        }
    }
}

/// A multipart request which writes all fields at once upon being provided an output stream.
///
/// Sacrifices static dispatch for support for dynamic construction. Reusable.
///
/// ####Lifetimes
/// * `'n`: Lifetime for field **n**ames; will only escape this struct in `LazyIoError<'n>`.
/// * `'d`: Lifetime for **d**ata: will only escape this struct in `PreparedFields<'d>`.
#[derive(Debug, Default)]
pub struct Multipart<'n, 'd> {
    fields: Vec<Field<'n, 'd>>,
}

impl<'n, 'd> Multipart<'n, 'd> {
    /// Initialize a new lazy dynamic request. 
    pub fn new() -> Self {
        Default::default()
    }

    /// Add a text field to this request. 
    pub fn add_text<N, T>(&mut self, name: N, text: T) -> &mut Self where N: Into<Cow<'n, str>>, T: Into<Cow<'d, str>> {
        self.fields.push(
            Field {
                name: name.into(),
                data: Data::Text(text.into())
            }
        );

        self
    }

    /// Add a file field to this request.
    ///
    /// ### Note
    /// Does not check if `path` exists.
    pub fn add_file<N, P>(&mut self, name: N, path: P) -> &mut Self where N: Into<Cow<'n, str>>, P: IntoCowPath<'d> {
        self.fields.push(
            Field {
                name: name.into(),
                data: Data::File(path.into_cow_path()),
            }
        );

        self
    }

    /// Add a generic stream field to this request,
    pub fn add_stream<N, R, F>(&mut self, name: N, stream: R, filename: Option<F>, mime: Option<Mime>) -> &mut Self where N: Into<Cow<'n, str>>, R: Read + 'd, F: Into<Cow<'n, str>> {
        self.fields.push(
            Field {
                name: name.into(),
                data: Data::Stream(Stream {
                    content_type: mime,
                    filename: filename.map(|f| f.into()),
                    stream: Box::new(stream)
                }),
            }
        );

        self
    }

    /// Convert `req` to `HttpStream`, write out the fields in this request, and finish the
    /// request, returning the response if successful, or the first error encountered.
    pub fn send<R: HttpRequest>(&mut self, req: R) -> Result<< R::Stream as HttpStream >::Response, LazyError<'n, < R::Stream as HttpStream >::Error>> {
        let (boundary, stream) = try_lazy!(super::open_stream(req, None));
        let mut writer = MultipartWriter::new(stream, boundary);

        for mut field in self.fields.drain(..) {
            try_lazy!(field.name, field.write_out(&mut writer));
        }

        try_lazy!(writer.finish()).finish().map_err(LazyError::without_field)
    }

    /// Export the multipart data contained in this lazy request as an adaptor which implements `Read`.
    ///
    /// A certain amount of field data will be buffered. See
    /// [`prepare_threshold()`](#method.prepare_threshold) for more information on this behavior.
    pub fn prepare(&mut self) -> LazyIoResult<'n, PreparedFields<'d>> {
        self.prepare_threshold(Some(DEFAULT_BUFFER_THRESHOLD))
    }

    /// Export the multipart data contained in this lazy request to an adaptor which implements `Read`.
    ///
    /// #### Buffering
    /// For efficiency, text and file fields smaller than `buffer_threshold` are copied to an in-memory buffer. If `None`,
    /// all fields are copied to memory.
    pub fn prepare_threshold(&mut self, buffer_threshold: Option<u64>) -> LazyIoResult<'n, PreparedFields<'d>> {
        let boundary = super::gen_boundary();
        PreparedFields::from_fields(&mut self.fields, boundary.into(), buffer_threshold)
    }
}

const DEFAULT_BUFFER_THRESHOLD: u64 = 8 * 1024;

#[derive(Debug)]
struct Field<'n, 'd> {
    name: Cow<'n, str>,
    data: Data<'n, 'd>,
}

impl<'n, 'd> Field<'n, 'd> {
    fn write_out<W: Write>(&mut self, writer: &mut MultipartWriter<W>) -> io::Result<()> {
        match self.data {
            Data::Text(ref text) => writer.write_text(&self.name, text),
            Data::File(ref path) => writer.write_file(&self.name, path),
            Data::Stream(ref mut stream) =>
                writer.write_stream(
                    &mut stream.stream,
                    &self.name,
                    stream.filename.as_ref().map(|f| &**f),
                    stream.content_type.clone(),
                ),
        }
    }
}

enum Data<'n, 'd> {
    Text(Cow<'d, str>),
    File(Cow<'d, Path>),
    Stream(Stream<'n, 'd>),
}

impl<'n, 'd> fmt::Debug for Data<'n, 'd> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            Data::Text(ref text) => write!(f, "Data::Text({:?})", text),
            Data::File(ref path) => write!(f, "Data::File({:?})", path),
            Data::Stream(_) => f.write_str("Data::Stream(Box<Read>"),
        }
    }
}

struct Stream<'n, 'd> {
    filename: Option<Cow<'n, str>>,
    content_type: Option<Mime>,
    stream: Box<Read + 'd>,
}

/// The result of [`Multipart::prepare()`](struct.Multipart.html#method.prepare) or
/// `Multipart::prepare_threshold()`. Implements `Read`, contains the entire request body.
pub struct PreparedFields<'d> {
    fields: vec::IntoIter<PreparedField<'d>>,
    next_field: Option<PreparedField<'d>>,
    boundary: String,
    content_len: Option<u64>,
}

impl<'d> PreparedFields<'d> {
    fn from_fields<'n>(fields: &mut Vec<Field<'n, 'd>>, boundary: String, buffer_threshold: Option<u64>) -> Result<Self, LazyIoError<'n>> {
        let buffer_threshold = buffer_threshold.unwrap_or(::std::u64::MAX);

        debug!("Field count: {}", fields.len());

        let mut prep_fields = Vec::new();

        let mut fields = fields.drain(..).peekable();

        {
            let mut writer = MultipartWriter::new(Vec::new(), &*boundary);

            while fields.peek().is_some() {
                if let Some(rem) = try!(write_fields(&mut writer, &mut fields, buffer_threshold)) {
                    let contiguous = mem::replace(writer.inner_mut(), Vec::new());
                    prep_fields.push(PreparedField::Partial(Cursor::new(contiguous), rem));
                }
            }

            let contiguous = writer.finish().unwrap();

            prep_fields.push(PreparedField::Contiguous(Cursor::new(contiguous)));
        }

        let content_len = get_content_len(&prep_fields);

        debug!("Prepared fields len: {}", prep_fields.len());

        Ok(PreparedFields {
            fields: prep_fields.into_iter(),
            next_field: None,
            boundary: boundary,
            content_len: content_len,
        })
    }

    /// Get the content-length value for this set of fields, if applicable (all fields are sized,
    /// i.e. not generic streams).
    pub fn content_len(&self) -> Option<u64> {
        self.content_len
    }

    /// Get the boundary that was used to serialize the request.
    pub fn boundary(&self) -> &str {
        &self.boundary
    }
}

impl<'d> Read for PreparedFields<'d> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.len() == 0 {
            debug!("PreparedFields::read() was passed a zero-sized buffer.");
            return Ok(0);
        }

        let mut total_read = 0;

        while total_read < buf.len() {
            if let None = self.next_field {
                self.next_field = self.fields.next();
            }

            let buf = &mut buf[total_read..];

            let num_read = if let Some(ref mut field) = self.next_field {
                try!(field.read(buf))
            } else {
                break;
            };

            if num_read == 0 {
                self.next_field = None;
            }

            total_read += num_read;
        }

        Ok(total_read)
    }
}

fn get_content_len(fields: &[PreparedField]) -> Option<u64> {
    let mut content_len = 0;

    for field in fields {
        match *field {
            PreparedField::Contiguous(ref vec) => content_len += vec.get_ref().len() as u64,
            PreparedField::Partial(..) => return None,
        }
    }

    Some(content_len)
}

fn write_fields<'n, 'd, I: Iterator<Item = Field<'n, 'd>>>(writer: &mut MultipartWriter<Vec<u8>>, fields: I, buffer_threshold: u64)
    -> LazyIoResult<'n, Option<Box<Read + 'd>>> {
    for field in fields {
        match field.data {
            Data::Text(text) => if text.len() as u64 <= buffer_threshold {
                try_lazy!(field.name, writer.write_text(&field.name, &*text));
            } else {
                try_lazy!(field.name, writer.write_field_headers(&field.name, None, None));
                return Ok(Some(Box::new(io::Cursor::new(CowStrAsRef(text)))));
            },
            Data::File(path) => {
                let (content_type, filename) = super::mime_filename(&*path);
                let mut file = try_lazy!(field.name, File::open(&*path));
                let len = try_lazy!(field.name, file.metadata()).len();

                if len <= buffer_threshold {
                    try_lazy!(field.name, writer.write_stream(&mut file, &field.name, filename, Some(content_type)));
                } else {
                    return Ok(Some(Box::new(file)));
                }
            },
            Data::Stream(stream) => {
                let content_type = stream.content_type.or_else(|| Some(::mime_guess::octet_stream()));
                let filename = stream.filename.as_ref().map(|f| &**f);
                try_lazy!(field.name, writer.write_field_headers(&field.name, filename, content_type));
                return Ok(Some(stream.stream));
            },
        }
    }

    Ok(None)
}

#[doc(hidden)]
pub enum PreparedField<'d> {
    Contiguous(io::Cursor<Vec<u8>>),
    Partial(io::Cursor<Vec<u8>>, Box<Read + 'd>),
}

impl<'d> Read for PreparedField<'d> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match *self {
            PreparedField::Contiguous(ref mut vec) => vec.read(buf),
            PreparedField::Partial(ref mut cursor, ref mut remainder) => {
                match cursor.read(buf) {
                    Ok(0) => remainder.read(buf),
                    res => res,
                }
            },
        }
    }
}

impl<'d> fmt::Debug for PreparedField<'d> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            PreparedField::Contiguous(ref bytes) => {
                if log_enabled!(LogLevel::Trace) {
                    write!(f, "PreparedField::Contiguous(\n{:?}\n)", String::from_utf8_lossy(bytes.get_ref()))
                } else {
                    write!(f, "PreparedField::Contiguous(len: {})", bytes.get_ref().len())
                }
            },
            PreparedField::Partial(ref bytes, _) => {
                if log_enabled!(LogLevel::Trace) {
                    write!(f, "PreparedField::Partial(\n{:?}\n, Box<Read>)", String::from_utf8_lossy(bytes.get_ref()))
                } else {
                    write!(f, "PreparedField::Partial(len: {}, Box<Read>)", bytes.get_ref().len())
                }
            }
        }
    }
}

struct CowStrAsRef<'d>(Cow<'d, str>);

impl<'d> AsRef<[u8]> for CowStrAsRef<'d> {
    fn as_ref(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

/// Conversion trait necessary for `Multipart::add_file()` to accept borrowed or owned strings
/// and borrowed or owned paths
pub trait IntoCowPath<'a> {
    /// Self-explanatory, hopefully
    fn into_cow_path(self) -> Cow<'a, Path>;
}

impl<'a> IntoCowPath<'a> for Cow<'a, Path> {
    fn into_cow_path(self) -> Cow<'a, Path> {
        self
    }
}

impl IntoCowPath<'static> for PathBuf {
    fn into_cow_path(self) -> Cow<'static, Path> {
        self.into()
    }
}

impl<'a> IntoCowPath<'a> for &'a Path {
    fn into_cow_path(self) -> Cow<'a, Path> {
        self.into()
    }
}

impl IntoCowPath<'static> for String {
    fn into_cow_path(self) -> Cow<'static, Path> {
        PathBuf::from(self).into()
    }
}

impl<'a> IntoCowPath<'a> for &'a str {
    fn into_cow_path(self) -> Cow<'a, Path> {
        Path::new(self).into()
    }
}

#[cfg(feature = "hyper")]
mod hyper {
    use hyper::client::{Body, Client, IntoUrl, RequestBuilder, Response};
    use hyper::Result as HyperResult;

    impl<'n, 'd> super::Multipart<'n, 'd> {
        /// #### Feature: `hyper`
        /// Complete a POST request with the given `hyper::client::Client` and URL.
        /// 
        /// Supplies the fields in the body, optionally setting the content-length header if
        /// applicable (all added fields were text or files, i.e. no streams).
        pub fn client_request<U: IntoUrl>(&mut self, client: &Client, url: U) -> HyperResult<Response> {
            self.client_request_mut(client, url, |r| r)
        }

        /// #### Feature: `hyper`
        /// Complete a POST request with the given `hyper::client::Client` and URL;
        /// allows mutating the `hyper::client::RequestBuilder` via the passed closure.
        ///
        /// Note that the body, and the `ContentType` and `ContentLength` headers will be
        /// overwritten, either by this method or by Hyper.
        pub fn client_request_mut<U: IntoUrl, F: FnOnce(RequestBuilder) -> RequestBuilder>(&mut self, client: &Client, url: U,
                                                                                           mut_fn: F) -> HyperResult<Response> {
            let mut fields = match self.prepare() {
                Ok(fields) => fields,
                Err(err) => {
                    error!("Error preparing request: {}", err);
                    return Err(err.error.into());
                },
            };


            mut_fn(client.post(url))
                .header(::client::hyper::content_type(fields.boundary()))
                .body(fields.to_body())
                .send()
        }
    }

    impl<'d> super::PreparedFields<'d> {
        /// #### Feature: `hyper`
        /// Convert `self` to `hyper::client::Body`.
        pub fn to_body<'b>(&'b mut self) -> Body<'b> where 'd: 'b {
            if let Some(content_len) = self.content_len {
                Body::SizedBody(self, content_len)
            } else {
                Body::ChunkedBody(self)
            }
        }
    }
}

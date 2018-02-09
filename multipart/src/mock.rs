// Copyright 2016 `multipart` Crate Developers
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.
//! Mocked types for client-side and server-side APIs.
use std::cell::{Cell, RefCell};
use std::io::{self, Read, Write};
use std::sync::{Once, ONCE_INIT};
use std::{fmt, ptr, thread};

use log::{Metadata, Record};

use rand::{self, Rng, ThreadRng};

/// A mock implementation of `client::HttpRequest` which can spawn an `HttpBuffer`.
///
/// `client::HttpRequest` impl requires the `client` feature.
#[derive(Default, Debug)]
pub struct ClientRequest {
    boundary: Option<String>,
    content_len: Option<u64>,
}

#[cfg(feature = "client")]
impl ::client::HttpRequest for ClientRequest {
    type Stream = HttpBuffer;
    type Error = io::Error;

    fn apply_headers(&mut self, boundary: &str, content_len: Option<u64>) -> bool {
        self.boundary = Some(boundary.into());
        self.content_len = content_len;
        true
    }

    /// ## Panics
    /// If `apply_headers()` was not called.
    fn open_stream(self) -> Result<HttpBuffer, io::Error> {
        debug!("ClientRequest::open_stream called! {:?}", self);
        let boundary = self.boundary.expect("ClientRequest::set_headers() was not called!");

        Ok(HttpBuffer::new_empty(boundary, self.content_len))
    }
}


/// A writable buffer which stores the boundary and content-length, if provided.
///
/// Implements `client::HttpStream` if the `client` feature is enabled.
pub struct HttpBuffer {
    /// The buffer containing the raw bytes.
    pub buf: Vec<u8>,
    /// The multipart boundary.
    pub boundary: String,
    /// The value of the content-length header, if set.
    pub content_len: Option<u64>,
    rng: ThreadRng,
}

impl HttpBuffer {
    /// Create an empty buffer with the given boundary and optional content-length.
    pub fn new_empty(boundary: String, content_len: Option<u64>) -> HttpBuffer {
        Self::with_buf(Vec::new(), boundary, content_len)
    }

    /// Wrap the given buffer with the given boundary and optional content-length.
    pub fn with_buf(buf: Vec<u8>, boundary: String, content_len: Option<u64>) -> Self {
        HttpBuffer {
            buf: buf,
            boundary: boundary,
            content_len: content_len,
            rng: rand::thread_rng()
        }
    }

    /// Get a `ServerRequest` wrapping the data in this buffer.
    pub fn for_server(&self) -> ServerRequest {
        ServerRequest {
            data: &self.buf,
            boundary: &self.boundary,
            content_len: self.content_len,
            rng: rand::thread_rng(),
        }
    }
}

impl Write for HttpBuffer {
    /// To simulate a network connection, this will copy a random number of bytes
    /// from `buf` to the buffer.
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            debug!("HttpBuffer::write() was passed a zero-sized buffer.");
            return Ok(0);
        }

        // Simulate the randomness of a network connection by not always reading everything
        let len = self.rng.gen_range(1, buf.len() + 1);

        self.buf.write(&buf[..len])
    }

    fn flush(&mut self) -> io::Result<()> {
        self.buf.flush()
    }
}

#[cfg(feature = "client")]
impl ::client::HttpStream for HttpBuffer {
    type Request = ClientRequest;
    type Response = HttpBuffer;
    type Error = io::Error;

    /// Returns `Ok(self)`.
    fn finish(self) -> Result<Self, io::Error> { Ok(self) }
}

impl fmt::Debug for HttpBuffer {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("multipart::mock::HttpBuffer")
            .field("buf", &self.buf)
            .field("boundary", &self.boundary)
            .field("content_len", &self.content_len)
            .finish()
    }
}

/// A mock implementation of `server::HttpRequest` that can be read.
///
/// Implements `server::HttpRequest` if the `server` feature is enabled.
pub struct ServerRequest<'a> {
    /// Slice of the source `HttpBuffer::buf`
    pub data: &'a [u8],
    /// The multipart boundary.
    pub boundary: &'a str,
    /// The value of the content-length header, if set.
    pub content_len: Option<u64>,
    rng: ThreadRng,
}

impl<'a> ServerRequest<'a> {
    /// Create a new `ServerRequest` with the given data and boundary.
    ///
    /// Assumes `content_len: None`
    pub fn new(data: &'a [u8], boundary: &'a str) -> Self {
        ServerRequest {
            data: data,
            boundary: boundary,
            content_len: None,
            rng: rand::thread_rng(),
        }
    }
}

impl<'a> Read for ServerRequest<'a> {
    /// To simulate a network connection, this will copy a random number of bytes
    /// from the buffer to `out`.
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        if out.is_empty() {
            debug!("ServerRequest::read() was passed a zero-sized buffer.");
            return Ok(0);
        }

        // Simulate the randomness of a network connection by not always reading everything
        let len = self.rng.gen_range(1, out.len() + 1);
        self.data.read(&mut out[..len])
    }
}

#[cfg(feature = "server")]
impl<'a> ::server::HttpRequest for ServerRequest<'a> {
    type Body = Self;

    fn multipart_boundary(&self) -> Option<&str> { Some(self.boundary) }

    fn body(self) -> Self::Body {
        self
    }
}

/// A `Write` adapter that duplicates all data written to the inner writer as well as stdout.
pub struct StdoutTee<'s, W> {
    inner: W,
    stdout: io::StdoutLock<'s>,
}

impl<'s, W> StdoutTee<'s, W> {
    /// Constructor
    pub fn new(inner: W, stdout: &'s io::Stdout) -> Self {
        Self {
            inner, stdout: stdout.lock(),
        }
    }
}

impl<'s, W: Write> Write for StdoutTee<'s, W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write(buf)?;
        self.stdout.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush();
        self.stdout.flush()
    }
}

/// Capture all logging on this thread during `PanicLogger`'s lifetime.
///
/// When `PanicLogger` is dropped: if the thread is panicking, all captured logs are printed
/// to stdout, otherwise the captured logs are deleted.
///
/// RFC: break this functionality out into its own crate? Could also support nested invocations
/// (where each instance of `PanicLogger` only prints the logs captured during its lifetime)
/// but that's currently unused here.
pub fn log_on_panic() -> PanicLogger {
    ::log::set_max_level(::log::LevelFilter::Trace);

    if ::log::logger() as *const ::log::Log != &LOGGER as *const ::log::Log {
        ::log::set_logger(&LOGGER).expect("failed to set logger for `log_on_panic()`");
    }

    LOG_ENABLED.with(|flag| flag.set(true));

    PanicLogger(())
}

/// Struct that logs if the thread panics before it is dropped.
#[must_use]
pub struct PanicLogger(());

impl PanicLogger {
    /// Clear the buffered logs
    pub fn clear(self) {
        drop(self);
    }
}

impl Drop for PanicLogger {
    fn drop(&mut self) {
        LOG_ENABLED.with(|flag| flag.set(false));
        LOG.with(|log| {
            let mut log = log.borrow_mut();
            if thread::panicking() {
                println!("captured logs before panic:\n{}", *log);
            }
            log.clear();
        });
    }
}

static INIT_PANIC_LOGGER: Once = ONCE_INIT;

thread_local! {
    static LOG: RefCell<String> = RefCell::new(String::new());
    static LOG_ENABLED: Cell<bool> = Cell::new(bool);
}

static LOGGER: ThreadLocalLogger = ThreadLocalLogger;

struct ThreadLocalLogger;

impl ::log::Log for ThreadLocalLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        LOG_ENABLED.with(Cell::get)
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            LOG.with(|log| {
                // format the message before pushing it to `ptr` since the fmt
                // impls of the record arguments could try to log as well
                let msg = format!("{}:{}: {}\n", record.level(), record.target(), record.args());
                log.borrow_mut().push_str(&msg);
            });
        }
    }

    fn flush(&self) {}
}

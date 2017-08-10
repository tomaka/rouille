// Copyright (c) 2017 The Rouille developers
// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT
// license <LICENSE-MIT or http://opensource.org/licenses/MIT>,
// at your option. All files in the project carrying such
// notice may not be copied, modified, or distributed except
// according to those terms.

use std::ascii::AsciiExt;
use std::borrow::Cow;
use std::io::copy;
use std::io::Cursor;
use std::io::Error as IoError;
use std::io::ErrorKind;
use std::io::Read;
use std::mem;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::mpsc::{channel, Sender, Receiver};
use std::sync::mpsc::TryRecvError;
use std::str;
use arrayvec::ArrayString;
use httparse;
use itoa::write as itoa;
use mio::Ready;
use mio::Registration;
use mio::SetReadiness;

use socket_handler::Protocol;
use socket_handler::RegistrationState;
use socket_handler::SocketHandler;
use socket_handler::Update;
use socket_handler::UpdateResult;
use socket_handler::request_body_analyzer::RequestBodyAnalyzer;
use socket_handler::task_pool::TaskPool;
use Request;
use Response;

/// Handles the processing of a client connection.
pub struct Http1Handler {
    // The handler is a state machine.
    state: Http1HandlerState,

    // Address of the client. Necessary for the request objects.
    client_addr: SocketAddr,

    // Protocol of the original server. Necessary for the request objects.
    original_protocol: Protocol,

    // Object that handles the request and returns a response.
    handler: Arc<Mutex<FnMut(Request) -> Response + Send + 'static>>,

    // The pool where to dispatch the handler.
    task_pool: TaskPool,
}

// Current status of the handler.
enum Http1HandlerState {
    // A panic happened during the processing. In this state any call to `update` will panic.
    Poisonned,

    // The `pending_read_buffer` doesn't have enough bytes to contain the initial request line.
    WaitingForRqLine {
        // Offset within `pending_read_buffer` where new data is available. Everything before this
        // offset was already in `pending_read_buffer` the last time `update` returned.
        new_data_start: usize,
    },

    // The request line has been parsed (its informations are inside the variant), but the
    // `pending_read_buffer` doesn't have enough bytes to contain the headers.
    WaitingForHeaders {
        // Offset within `pending_read_buffer` where new data is available. Everything before this
        // offset was already in `pending_read_buffer` the last time `update` returned.
        new_data_start: usize,
        // HTTP method (eg. GET, POST, ...) parsed from the request line.
        method: ArrayString<[u8; 17]>,
        // URL requested by the HTTP client parsed from the request line.
        path: String,
        // HTTP version parsed from the request line.
        version: HttpVersion,
    },

    // The handler is currently being executed in the task pool and is streaming data.
    ExecutingHandler {
        // True if `Connection: close` was requested by the client as part of the headers.
        connection_close: bool,
        // Analyzes and decodes the client input.
        input_analyzer: RequestBodyAnalyzer,
        // Used to send buffers containing the body of the request. `None` if no more data.
        input_data: Option<Sender<Vec<u8>>>,
        // Contains blocks of output data streamed by the handler. Closed when the handler doesn't
        // have any more data to send.
        response_getter: Receiver<Vec<u8>>,
        // Registration that is triggered by the background thread whenever some data is available
        // in `response_getter`.
        registration: (Arc<Registration>, RegistrationState),
    },

    // Happens after a request with `Connection: close`. The connection is considered as closed by
    // the handler and nothing more will be processed.
    Closed,
}

impl Http1Handler {
    /// Starts handling a new HTTP client connection.
    ///
    /// `client_addr` and `original_protocol` are necessary for building the `Request` objects.
    /// `task_pool` and `handler` indicate how the requests must be processed.
    pub fn new<F>(client_addr: SocketAddr, original_protocol: Protocol, task_pool: TaskPool,
                  handler: F) -> Http1Handler
        where F: FnMut(Request) -> Response + Send + 'static
    {
        Http1Handler {
            state: Http1HandlerState::WaitingForRqLine { new_data_start: 0 },
            client_addr: client_addr,
            original_protocol: original_protocol,
            handler: Arc::new(Mutex::new(handler)),
            task_pool: task_pool,
        }
    }
}

impl SocketHandler for Http1Handler {
    fn update(&mut self, update: &mut Update) -> UpdateResult {
        loop {
            match mem::replace(&mut self.state, Http1HandlerState::Poisonned) {
                Http1HandlerState::Poisonned => {
                    panic!("Poisonned request handler");
                },

                Http1HandlerState::WaitingForRqLine { new_data_start } => {
                    // Try to find a \r\n in the buffer.
                    let off = new_data_start.saturating_sub(1);
                    let rn = update.pending_read_buffer[off..].windows(2)
                                                              .position(|w| w == b"\r\n");
                    if let Some(rn) = rn {
                        // Found a request line!
                        let method;
                        let path;
                        let version;
                        {
                            let (method_raw, path_raw, version_raw) = match parse_request_line(&update.pending_read_buffer[..rn]) {
                                Ok(v) => v,
                                Err(_) => {
                                    write_status_and_headers(&mut update.pending_write_buffer, 400, &[], Some(0));
                                    self.state = Http1HandlerState::Closed;
                                    break UpdateResult {
                                        registration: None,
                                        close_read: true,
                                        write_flush_suggested: true,
                                    };
                                },
                            };

                            method = match ArrayString::from(method_raw) {
                                Ok(m) => m,
                                Err(_) => {
                                    write_status_and_headers(&mut update.pending_write_buffer, 501, &[], Some(0));
                                    self.state = Http1HandlerState::Closed;
                                    break UpdateResult {
                                        registration: None,
                                        close_read: true,
                                        write_flush_suggested: true,
                                    };
                                },
                            };

                            path = path_raw.to_owned();
                            version = version_raw;
                        };

                        // Remove the request line from the head of the buffer.
                        let cut_len = update.pending_read_buffer.len() - (rn + 2);
                        for n in 0 .. cut_len {
                            update.pending_read_buffer[n] = update.pending_read_buffer[n + rn + 2];
                        }
                        update.pending_read_buffer.resize(cut_len, 0);

                        self.state = Http1HandlerState::WaitingForHeaders {
                            new_data_start: 0,
                            method,
                            path,
                            version
                        };

                    } else {
                        // No full request line in the buffer yet.
                        // TODO: put a limit on the buffer size
                        self.state = Http1HandlerState::WaitingForRqLine {
                            new_data_start: update.pending_read_buffer.len(),
                        };

                        break UpdateResult {
                            registration: None,
                            close_read: false,
                            write_flush_suggested: false,
                        };
                    }
                },

                Http1HandlerState::WaitingForHeaders { new_data_start, method, path, version } => {
                    // Try to find a `\r\n\r\n` in the buffer which would indicate the end of the
                    // headers.
                    let off = new_data_start.saturating_sub(3);
                    let rnrn = update.pending_read_buffer[off..].windows(4)
                                                                .position(|w| w == b"\r\n\r\n");
                    if let Some(rnrn) = rnrn {
                        // Found headers! Parse them.
                        let headers: Vec<(String, String)> = {
                            let mut out_headers = Vec::new();
                            let mut headers = [httparse::EMPTY_HEADER; 32];
                            let (_, parsed_headers) = httparse::parse_headers(&update.pending_read_buffer, &mut headers).unwrap().unwrap();        // TODO:
                            for parsed in parsed_headers {
                                out_headers.push((parsed.name.to_owned(), String::from_utf8_lossy(parsed.value).into()));      // TODO: wrong
                            }
                            out_headers
                        };

                        // Remove the headers from the head of the buffer.
                        let cut_len = update.pending_read_buffer.len() - (off + rnrn + 4);
                        for n in 0 .. cut_len {
                            update.pending_read_buffer[n] = update.pending_read_buffer[n + off + rnrn + 4];
                        }
                        update.pending_read_buffer.resize(cut_len, 0);

                        let input_analyzer = {
                            let iter = headers
                                .iter()
                                .map(|&(ref h, ref v)| (h.as_str(), v.as_str()));
                            RequestBodyAnalyzer::new(iter)
                        };

                        // We now create a new task for our task pool in which the request is
                        // built, the handler is called, and the response is sent as Vecs through
                        // a channel.
                        let (data_out_tx, data_out_rx) = channel();
                        let (data_in_tx, data_in_rx) = channel();
                        let (registration, set_ready) = Registration::new2();
                        spawn_handler_task(&self.task_pool, self.handler.clone(), method, path,
                                           headers, self.original_protocol,
                                           self.client_addr.clone(), data_out_tx, data_in_rx,
                                           set_ready);

                        self.state = Http1HandlerState::ExecutingHandler {
                            connection_close: false,        // TODO:
                            input_analyzer: input_analyzer,
                            input_data: Some(data_in_tx),
                            response_getter: data_out_rx,
                            registration: (Arc::new(registration), RegistrationState::FirstTime),
                        };

                    } else {
                        // No full headers in the buffer yet.
                        // TODO: put a limit on the buffer size
                        self.state = Http1HandlerState::WaitingForHeaders {
                            new_data_start: update.pending_read_buffer.len(),
                            method,
                            path,
                            version
                        };

                        break UpdateResult {
                            registration: None,
                            close_read: false,
                            write_flush_suggested: false,
                        };
                    }
                },

                Http1HandlerState::ExecutingHandler { connection_close, mut input_data,
                                                      mut input_analyzer, response_getter,
                                                      registration } =>
                {
                    {
                        let analysis = input_analyzer.feed(&mut update.pending_read_buffer);
                        if analysis.body_data >= 1 {
                            // TODO: more optimal
                            let body_data = update.pending_read_buffer[0 .. analysis.body_data].to_owned();
                            update.pending_read_buffer = update.pending_read_buffer[analysis.body_data + analysis.unused_trailing..].to_owned();
                            let _ = input_data.as_mut().unwrap().send(body_data);
                        } else {
                            assert_eq!(analysis.unused_trailing, 0);
                        }
                        if analysis.finished {
                            input_data = None;
                        }
                    }

                    match response_getter.try_recv() {
                        Ok(mut data) => {
                            // Got some data for the response.
                            if update.pending_write_buffer.is_empty() {
                                update.pending_write_buffer = data;
                            } else {
                                update.pending_write_buffer.append(&mut data);
                            }
                            self.state = Http1HandlerState::ExecutingHandler {
                                connection_close: connection_close,
                                input_data: input_data,
                                input_analyzer: input_analyzer,
                                response_getter: response_getter,
                                registration: registration,
                            };
                        },
                        Err(TryRecvError::Disconnected) => {
                            // The handler has finished streaming the response.
                            if connection_close {
                                self.state = Http1HandlerState::Closed;
                            } else {
                                self.state = Http1HandlerState::WaitingForRqLine {
                                    new_data_start: 0
                                };
                                break UpdateResult {
                                    registration: None,
                                    close_read: false,
                                    write_flush_suggested: true,
                                };
                            }
                        },
                        Err(TryRecvError::Empty) => {
                            // Spurious wakeup.
                            self.state = Http1HandlerState::ExecutingHandler {
                                connection_close: connection_close,
                                input_data: input_data,
                                input_analyzer: input_analyzer,
                                response_getter: response_getter,
                                registration: (registration.0.clone(), RegistrationState::Reregister),
                            };
                            break UpdateResult {
                                registration: Some(registration),
                                close_read: false,
                                write_flush_suggested: false,
                            };
                        },
                    }
                },

                Http1HandlerState::Closed => {
                    self.state = Http1HandlerState::Closed;
                    break UpdateResult {
                        registration: None,
                        close_read: true,
                        write_flush_suggested: true,
                    };
                },
            }
        }
    }
}

// Starts the task of handling a request.
fn spawn_handler_task(task_pool: &TaskPool,
                      handler: Arc<Mutex<FnMut(Request) -> Response + Send + 'static>>,
                      method: ArrayString<[u8; 17]>, path: String,
                      headers: Vec<(String, String)>, original_protocol: Protocol,
                      remote_addr: SocketAddr, data_out_tx: Sender<Vec<u8>>,
                      data_in_rx: Receiver<Vec<u8>>, set_ready: SetReadiness)
{
    let https = original_protocol == Protocol::Https;

    struct ReadWrapper(Receiver<Vec<u8>>, Cursor<Vec<u8>>);
    impl Read for ReadWrapper {
        fn read(&mut self, buf: &mut [u8]) -> Result<usize, IoError> {
            let initial_buf_len = buf.len() as u64;
            let mut total_written = 0;
            let mut buf = Cursor::new(buf);

            total_written += copy(&mut self.1, &mut buf).unwrap();
            debug_assert!(total_written <= initial_buf_len);
            if total_written == initial_buf_len {
                return Ok(total_written as usize);
            }

            match self.0.recv() {
                Ok(data) => self.1 = Cursor::new(data),
                Err(_) => return Ok(total_written as usize),
            };

            total_written += copy(&mut self.1, &mut buf).unwrap();
            debug_assert!(total_written <= initial_buf_len);
            if total_written == initial_buf_len {
                return Ok(total_written as usize);
            }

            loop {
                match self.0.try_recv() {
                    Ok(data) => self.1 = Cursor::new(data),
                    Err(_) => return Ok(total_written as usize),
                };

                total_written += copy(&mut self.1, &mut buf).unwrap();
                debug_assert!(total_written <= initial_buf_len);
                if total_written == initial_buf_len {
                    return Ok(total_written as usize);
                }
            }
        }
    }

    let reader = ReadWrapper(data_in_rx, Cursor::new(Vec::new()));

    task_pool.spawn(move || {
        let request = Request {
            method: method,
            url: path,
            headers: headers,
            https: https,
            data: Arc::new(Mutex::new(Some(Box::new(reader)))),
            remote_addr: remote_addr,
        };

        let response = {
            let mut handler = handler.lock().unwrap();
            (&mut *handler)(request)
        };
        assert!(response.upgrade.is_none());        // TODO: unimplemented

        let (mut body_data, body_size) = response.data.into_reader_and_size();

        let mut out_buffer = Vec::new();
        write_status_and_headers(&mut out_buffer,
                                 response.status_code,
                                 &response.headers,
                                 body_size);

        match data_out_tx.send(out_buffer) {
            Ok(_) => (),
            Err(_) => return,
        };

        let _ = set_ready.set_readiness(Ready::readable());

        loop {
            let mut out_data = vec![0; 256];
            match body_data.read(&mut out_data) {
                Ok(0) => break,
                Ok(n) => out_data.truncate(n),
                Err(ref e) if e.kind() == ErrorKind::Interrupted => {},
                Err(_) => {
                    // Handle errors by silently stopping the stream.
                    // TODO: better way?
                    return;
                },
            };

            match data_out_tx.send(out_data) {
                Ok(_) => (),
                Err(_) => return,
            };
            let _ = set_ready.set_readiness(Ready::readable());
        }
    });
}

// HTTP version (usually 1.0 or 1.1).
#[derive(Debug, Clone, PartialEq, Eq)]
struct HttpVersion(pub u8, pub u8);

// Parses a "HTTP/1.1" string.
// TODO: handle [u8] correctly
fn parse_http_version(version: &str) -> Result<HttpVersion, ()> {
    let mut elems = version.splitn(2, '/');

    elems.next();
    let vers = match elems.next() {
        Some(v) => v,
        None => return Err(()),
    };

    let mut elems = vers.splitn(2, '.');
    let major = elems.next().and_then(|n| n.parse().ok());
    let minor = elems.next().and_then(|n| n.parse().ok());

    match (major, minor) {
        (Some(ma), Some(mi)) => Ok(HttpVersion(ma, mi)),
        _ => return Err(()),
    }
}

// Parses the request line of the request.
// eg. GET / HTTP/1.1
// TODO: handle [u8] correctly
fn parse_request_line(line: &[u8]) -> Result<(&str, &str, HttpVersion), ()> {
    let line = str::from_utf8(line).unwrap();       // TODO:
    let mut words = line.split(' ');

    let method = words.next();
    let path = words.next();
    let version = words.next();

    let (method, path, version) = match (method, path, version) {
        (Some(m), Some(p), Some(v)) => (m, p, v),
        _ => return Err(())
    };

    let version = parse_http_version(version)?;
    Ok((method, path, version))
}

// Writes the status line and headers of the response to `out`.
fn write_status_and_headers(mut out: &mut Vec<u8>, status_code: u16,
                            headers: &[(Cow<'static, str>, Cow<'static, str>)],
                            body_size: Option<usize>)
{
    out.extend_from_slice(b"HTTP/1.1 ");
    itoa(&mut out, status_code).unwrap();
    out.push(b' ');
    out.extend_from_slice(default_reason_phrase(status_code).as_bytes());
    out.extend_from_slice(b"\r\n");

    let mut found_server_header = false;
    let mut found_date_header = false;
    for &(ref header, ref value) in headers {
        if !found_server_header && header.eq_ignore_ascii_case("Server") {
            found_server_header = true;
        }
        if !found_date_header && header.eq_ignore_ascii_case("Date") {
            found_date_header = true;
        }

        // Some headers can't be written with the response, as they are too "low-level".
        if header.eq_ignore_ascii_case("Content-Length") ||
            header.eq_ignore_ascii_case("Transfer-Encoding") ||
            header.eq_ignore_ascii_case("Connection") ||
            header.eq_ignore_ascii_case("Trailer")
        {
            continue;
        }

        out.extend_from_slice(header.as_bytes());
        out.extend_from_slice(b": ");
        out.extend_from_slice(value.as_bytes());
        out.extend_from_slice(b"\r\n");
    }

    if !found_server_header {
        out.extend_from_slice(b"Server: rouille\r\n");
    }
    if !found_date_header {
        out.extend_from_slice(b"Date: TODO\r\n");      // TODO:
    }

    out.extend_from_slice(b"Content-Length: ");
    itoa(&mut out, body_size.unwrap()).unwrap();      // TODO: don't unwrap body_size
    out.extend_from_slice(b"\r\n");
    out.extend_from_slice(b"\r\n");
}

// Returns the phrase corresponding to a status code.
fn default_reason_phrase(status_code: u16) -> &'static str {
    match status_code {
        100 => "Continue",
        101 => "Switching Protocols",
        102 => "Processing",
        118 => "Connection timed out",
        200 => "OK",
        201 => "Created",
        202 => "Accepted",
        203 => "Non-Authoritative Information",
        204 => "No Content",
        205 => "Reset Content",
        206 => "Partial Content",
        207 => "Multi-Status",
        210 => "Content Different",
        300 => "Multiple Choices",
        301 => "Moved Permanently",
        302 => "Found",
        303 => "See Other",
        304 => "Not Modified",
        305 => "Use Proxy",
        307 => "Temporary Redirect",
        400 => "Bad Request",
        401 => "Unauthorized",
        402 => "Payment Required",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        406 => "Not Acceptable",
        407 => "Proxy Authentication Required",
        408 => "Request Time-out",
        409 => "Conflict",
        410 => "Gone",
        411 => "Length Required",
        412 => "Precondition Failed",
        413 => "Request Entity Too Large",
        414 => "Reques-URI Too Large",
        415 => "Unsupported Media Type",
        416 => "Request range not satisfiable",
        417 => "Expectation Failed",
        500 => "Internal Server Error",
        501 => "Not Implemented",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Time-out",
        505 => "HTTP Version not supported",
        _ => "Unknown"
    }
}

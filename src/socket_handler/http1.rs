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
use std::io::Read;
use std::mem;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::mpsc::{channel, Receiver};
use std::str;
use arrayvec::ArrayString;
use httparse;
use itoa::write as itoa;
use mio::Ready;
use mio::Registration;

use socket_handler::Protocol;
use socket_handler::SocketHandler;
use socket_handler::Update;
use socket_handler::task_pool::TaskPool;
use Request;
use Response;

/// Handles the processing of a client connection.
pub struct Http1Handler {
    // The handler is a state machine.
    state: Http1HandlerState,

    // Address of the client. Passed to the request object.
    client_addr: SocketAddr,

    // Protocol of the original server. Passed to the request object.
    original_protocol: Protocol,

    // Object that handles the request and returns a response.
    handler: Arc<Mutex<FnMut(Request) -> Response + Send + 'static>>,
    
    // The pool where to dispatch the handler.
    task_pool: TaskPool,
}

enum Http1HandlerState {
    Poisonned,
    WaitingForRqLine {
        // Offset within `pending_read_buffer` where new data is available. Everything before this
        // offset was already in `pending_read_buffer` the last time `update` returned.
        new_data_start: usize,
    },
    WaitingForHeaders {
        // Offset within `pending_read_buffer` where new data is available. Everything before this
        // offset was already in `pending_read_buffer` the last time `update` returned.
        new_data_start: usize,
        method: ArrayString<[u8; 16]>,
        path: String,
        version: HttpVersion,
    },
    ExecutingHandler {
        response_getter: Receiver<Response>,
        registration: Arc<Registration>,
    },
    SendingResponse {
        data: Box<Read + Send>,
    },
    Closed,
}

impl Http1Handler {
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
    fn update(&mut self, update: &mut Update) {
        loop {
            match mem::replace(&mut self.state, Http1HandlerState::Poisonned) {
                Http1HandlerState::Poisonned => {
                    panic!("Poisonned request handler");
                },

                Http1HandlerState::WaitingForRqLine { new_data_start } => {
                    let off = new_data_start.saturating_sub(1);
                    if let Some(rn) = update.pending_read_buffer[off..].windows(2).position(|w| w == b"\r\n") {
                        let (method, path, version) = {
                            let (method, path, version) = parse_request_line(&update.pending_read_buffer[..rn]).unwrap();       // TODO: error
                            let method = ArrayString::from(method).unwrap();        // TODO: error
                            (method, path.to_owned(), version)
                        };

                        // Remove the first `rn + 2` bytes of `update.pending_read_buffer`
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
                        self.state = Http1HandlerState::WaitingForRqLine {
                            new_data_start: update.pending_read_buffer.len(),
                        };
                        break;
                    }
                },

                Http1HandlerState::WaitingForHeaders { new_data_start, method, path, version } => {
                    let off = new_data_start.saturating_sub(3);
                    if let Some(rnrn) = update.pending_read_buffer[off..].windows(4).position(|w| w == b"\r\n\r\n") {
                        let headers = {
                            let mut out_headers = Vec::new();
                            let mut headers = [httparse::EMPTY_HEADER; 32];
                            let (_, parsed_headers) = httparse::parse_headers(&update.pending_read_buffer, &mut headers).unwrap().unwrap();        // TODO:
                            for parsed in parsed_headers {
                                out_headers.push((parsed.name.to_owned(), String::from_utf8_lossy(parsed.value).into()));      // TODO: wrong
                            }
                            out_headers
                        };

                        // Remove the first `off + rnrn + 4` bytes of `update.pending_read_buffer`
                        let cut_len = update.pending_read_buffer.len() - (off + rnrn + 4);
                        for n in 0 .. cut_len {
                            update.pending_read_buffer[n] = update.pending_read_buffer[n + off + rnrn + 4];
                        }
                        update.pending_read_buffer.resize(cut_len, 0);

                        // TODO: yeah, don't spawn threads left and right
                        let (registration, set_ready) = Registration::new2();
                        let registration = Arc::new(registration);
                        let handler = self.handler.clone();
                        let https = self.original_protocol == Protocol::Https;
                        let remote_addr = self.client_addr.clone();
                        let (tx, rx) = channel();
                        self.task_pool.spawn(move || {
                            let request = Request {
                                method: method,
                                url: path,
                                headers: headers,
                                https: https,
                                data: Arc::new(Mutex::new(None)),       // FIXME:
                                remote_addr: remote_addr,
                            };

                            let mut handler = handler.lock().unwrap();
                            let response = (&mut *handler)(request);
                            let _ = tx.send(response);
                            let _ = set_ready.set_readiness(Ready::readable());
                        });

                        update.registration = Some(registration.clone());
                        self.state = Http1HandlerState::ExecutingHandler {
                            response_getter: rx,
                            registration: registration,
                        };
                        break;

                    } else {
                        self.state = Http1HandlerState::WaitingForHeaders {
                            new_data_start: update.pending_read_buffer.len(),
                            method,
                            path,
                            version
                        };
                        break;
                    }
                },

                Http1HandlerState::ExecutingHandler { response_getter, registration } => {
                    // TODO: write incoming data to request's reader
                    if let Ok(response) = response_getter.try_recv() {
                        assert!(response.upgrade.is_none());        // TODO:

                        let (body_data, body_size) = response.data.into_reader_and_size();
                        write_status_and_headers(&mut update.pending_write_buffer,
                                                 response.status_code,
                                                 &response.headers,
                                                 body_size);

                        self.state = Http1HandlerState::SendingResponse {
                            data: Box::new(body_data)
                        };

                    } else {
                        self.state = Http1HandlerState::ExecutingHandler { response_getter, registration };
                        break;
                    }
                },

                Http1HandlerState::SendingResponse { mut data } => {
                    // TODO: meh, this can block
                    data.read_to_end(&mut update.pending_write_buffer).unwrap();
                    self.state = Http1HandlerState::WaitingForRqLine { new_data_start: 0 };
                    break;
                },

                Http1HandlerState::Closed => {
                    debug_assert!(!update.accepts_read);
                    self.state = Http1HandlerState::Closed;
                    break;
                },
            }
        }
    }
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

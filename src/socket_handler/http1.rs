// Copyright (c) 2017 The Rouille developers
// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT
// license <LICENSE-MIT or http://opensource.org/licenses/MIT>,
// at your option. All files in the project carrying such
// notice may not be copied, modified, or distributed except
// according to those terms.

use std::io::Cursor;
use std::io::Read;
use std::io::Write;
use std::mem;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::mpsc::{channel, Receiver};
use std::str;
use std::thread;
use httparse;
use mio::Ready;
use mio::Registration;

use socket_handler::Update;
use Request;
use Response;

/// Handles the processing of a client connection.
pub struct Http1Handler {
    // The handler is a state machine.
    state: Http1HandlerState,

    // Address of the client. Will be extracted at some point during the handling.
    client_addr: SocketAddr,

    // Object that handles the request and returns a response.
    handler: Arc<Mutex<FnMut(Request) -> Response + Send + 'static>>,
}

enum Http1HandlerState {
    Poisonned,
    WaitingForRqLine,
    WaitingForHeaders {
        method: String,
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
    pub fn new<F>(client_addr: SocketAddr, handler: F) -> Http1Handler
        where F: FnMut(Request) -> Response + Send + 'static
    {
        Http1Handler {
            state: Http1HandlerState::WaitingForRqLine,
            client_addr: client_addr,
            handler: Arc::new(Mutex::new(handler)),
        }
    }

    pub fn update(&mut self, update: &mut Update) {
        loop {
            match mem::replace(&mut self.state, Http1HandlerState::Poisonned) {
                Http1HandlerState::Poisonned => {
                    panic!("Poisonned request handler");
                },

                Http1HandlerState::WaitingForRqLine => {
                    let off = update.new_data_start.saturating_sub(1);
                    if let Some(rn) = update.pending_read_buffer[off..].windows(2).position(|w| w == b"\r\n") {
                        let (method, path, version) = {
                            let (method, path, version) = parse_request_line(&update.pending_read_buffer[..rn]).unwrap();       // TODO: error
                            (method.to_owned(), path.to_owned(), version)
                        };
                        // TODO: don't reallocate a Vec
                        update.pending_read_buffer = update.pending_read_buffer[rn + 2..].to_owned();
                        self.state = Http1HandlerState::WaitingForHeaders { method, path, version };
                    } else {
                        self.state = Http1HandlerState::WaitingForRqLine;
                        break;
                    }
                },

                Http1HandlerState::WaitingForHeaders { method, path, version } => {
                    let off = update.new_data_start.saturating_sub(3);
                    if let Some(rnrn) = update.pending_read_buffer[off..].windows(4).position(|w| w == b"\r\n\r\n") {
                        let headers = {
                            let mut out_headers = Vec::new();
                            let mut headers = [httparse::EMPTY_HEADER; 32];
                            let (_, parsed_headers) = httparse::parse_headers(&update.pending_read_buffer, &mut headers).unwrap().unwrap();        // TODO:
                            for parsed in parsed_headers {
                                out_headers.push((parsed.name.to_owned(), String::from_utf8_lossy(parsed.value).into()));      // TODO: wrong
                            }
                            println!("{:?}", out_headers);
                            out_headers
                        };

                        // TODO: don't reallocate a Vec
                        update.pending_read_buffer = update.pending_read_buffer[off + rnrn + 4..].to_owned();

                        // TODO: yeah, don't spawn threads left and right
                        let (registration, set_ready) = Registration::new2();
                        let registration = Arc::new(registration);
                        let handler = self.handler.clone();
                        let remote_addr = self.client_addr.clone();
                        let (tx, rx) = channel();
                        thread::spawn(move || {
                            let request = Request {
                                method: method,
                                url: path,
                                headers: headers,
                                https: false,
                                data: Arc::new(Mutex::new(None)),       // FIXME:
                                remote_addr: remote_addr,
                            };

                            let mut handler = handler.lock().unwrap();
                            let response = (&mut *handler)(request);
                            let _ = tx.send(response);  
                            ::std::thread::sleep(::std::time::Duration::from_millis(500));  // TODO: remove
                            let _ = set_ready.set_readiness(Ready::readable());
                        });

                        update.registration = Some(registration.clone());
                        self.state = Http1HandlerState::ExecutingHandler {
                            response_getter: rx,
                            registration: registration,
                        };
                        break;

                    } else {
                        self.state = Http1HandlerState::WaitingForHeaders { method, path, version };
                        break;
                    }
                },

                Http1HandlerState::ExecutingHandler { response_getter, registration } => {
                    // TODO: write incoming data to request's reader
                    if let Ok(response) = response_getter.try_recv() {
                        assert!(response.upgrade.is_none());        // TODO:

                        let (body_data, body_size) = response.data.into_reader_and_size();

                        let mut headers_data = Vec::new();
                        write!(headers_data, "HTTP/1.1 {} Ok\r\n", response.status_code).unwrap();
                        for (header, value) in response.headers {
                            write!(headers_data, "{}: {}\r\n", header, value).unwrap();
                        }
                        write!(headers_data, "Content-Length: {}\r\n", body_size.unwrap()).unwrap();        // TODO: don't unwrap body_size
                        write!(headers_data, "\r\n").unwrap();

                        let full_data = Cursor::new(headers_data).chain(body_data);

                        self.state = Http1HandlerState::SendingResponse {
                            data: Box::new(full_data)
                        };

                    } else {
                        self.state = Http1HandlerState::ExecutingHandler { response_getter, registration };
                        break;
                    }
                },

                Http1HandlerState::SendingResponse { mut data } => {
                    // TODO: meh, this can block
                    data.read_to_end(&mut update.pending_write_buffer).unwrap();
                    self.state = Http1HandlerState::WaitingForRqLine;
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

/// HTTP version (usually 1.0 or 1.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpVersion(pub u8, pub u8);

/// Parses a "HTTP/1.1" string.
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

/// Parses the request line of the request.
/// eg. GET / HTTP/1.1
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

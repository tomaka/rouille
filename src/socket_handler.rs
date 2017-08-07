// Copyright (c) 2017 The Rouille developers
// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT
// license <LICENSE-MIT or http://opensource.org/licenses/MIT>,
// at your option. All files in the project carrying such
// notice may not be copied, modified, or distributed except
// according to those terms.

use std::io::Cursor;
use std::io::ErrorKind;
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

use Request;
use Response;

/// Handles the processing of a client connection.
pub struct SocketHandler {
    // The handler is a state machine.
    state: SocketHandlerState,

    // Address of the client. Will be extracted at some point during the handling.
    client_addr: Option<SocketAddr>,

    // Object that handles the request and returns a response.
    handler: Option<Box<FnMut(Request) -> Response + Send + 'static>>,
}

enum SocketHandlerState {
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

/// Represents the communication between the `SocketHandler` and the outside.
///
/// The "outside" is supposed to fill `pending_read_buffer` with incoming data, and remove data
/// from `pending_write_buffer`, then call `update`.
pub struct Update {
    /// Filled by the handler user. Contains the data that comes from the client.
    pub pending_read_buffer: Vec<u8>,

    /// Offset within `pending_read_buffer` where new data is available. Everything before this
    /// offset was already in `pending_read_buffer` the last time `update` returned.
    pub new_data_start: usize,

    /// Set to false by the socket handler when it will no longer process incoming data. If both
    /// `accepts_read` is false and `pending_write_buffer` is empty, then you can drop the socket.
    pub accepts_read: bool,

    /// Filled by `SocketHandler::update()`. Contains the data that must be sent back to the
    /// client.
    pub pending_write_buffer: Vec<u8>,

    /// When set by the socket handler, it means that the user must call `update` when the
    /// `Registration` becomes ready. The user must then set it to 0. The registration is only ever
    /// used once.
    pub registration: Option<Arc<Registration>>,
}

impl Update {
    pub fn empty() -> Update {
        // TODO: don't create two Vecs for each socket
        Update {
            pending_read_buffer: Vec::new(),
            new_data_start: 0,
            accepts_read: true,
            pending_write_buffer: Vec::new(),
            registration: None,
        }
    }
}

impl SocketHandler {
    pub fn new<F>(client_addr: SocketAddr, handler: F) -> SocketHandler
        where F: FnMut(Request) -> Response + Send + 'static
    {
        SocketHandler {
            state: SocketHandlerState::WaitingForRqLine,
            client_addr: Some(client_addr),
            handler: Some(Box::new(handler)),
        }
    }

    pub fn update(&mut self, update: &mut Update) {
        loop {
            match mem::replace(&mut self.state, SocketHandlerState::Poisonned) {
                SocketHandlerState::Poisonned => {
                    panic!("Poisonned request handler");
                },

                SocketHandlerState::WaitingForRqLine => {
                    let off = update.new_data_start.saturating_sub(1);
                    if let Some(rn) = update.pending_read_buffer[off..].windows(2).position(|w| w == b"\r\n") {
                        let (method, path, version) = {
                            let (method, path, version) = parse_request_line(&update.pending_read_buffer[..rn]).unwrap();       // TODO: error
                            (method.to_owned(), path.to_owned(), version)
                        };
                        // TODO: don't reallocate a Vec
                        update.pending_read_buffer = update.pending_read_buffer[rn + 2..].to_owned();
                        self.state = SocketHandlerState::WaitingForHeaders { method, path, version };
                    } else {
                        self.state = SocketHandlerState::WaitingForRqLine;
                        break;
                    }
                },

                SocketHandlerState::WaitingForHeaders { method, path, version } => {
                    let off = update.new_data_start.saturating_sub(3);
                    if let Some(rnrn) = update.pending_read_buffer[off..].windows(4).position(|w| w == b"\r\n\r\n") {
                        {
                            let mut headers = [httparse::EMPTY_HEADER; 32];
                            httparse::parse_headers(&update.pending_read_buffer, &mut headers).unwrap();        // TODO:
                            println!("{:?}", headers);
                        }

                        // TODO: don't reallocate a Vec
                        update.pending_read_buffer = update.pending_read_buffer[off + rnrn + 4..].to_owned();

                        // TODO: yeah, don't spawn threads left and right
                        let (registration, set_ready) = Registration::new2();
                        let registration = Arc::new(registration);
                        let mut handler = self.handler.take().unwrap();
                        let remote_addr = self.client_addr.take().unwrap();
                        let (tx, rx) = channel();
                        thread::spawn(move || {
                            let request = Request {
                                method: method,
                                url: path,
                                headers: Vec::new(),
                                https: false,
                                data: Arc::new(Mutex::new(None)),       // FIXME:
                                remote_addr: remote_addr,
                            };

                            let response = handler(request);
                            let _ = tx.send(response);
                            ::std::thread::sleep(::std::time::Duration::from_millis(500));  // TODO: remove
                            let _ = set_ready.set_readiness(Ready::readable());
                        });

                        update.registration = Some(registration.clone());
                        self.state = SocketHandlerState::ExecutingHandler {
                            response_getter: rx,
                            registration: registration,
                        };
                        break;

                    } else {
                        self.state = SocketHandlerState::WaitingForHeaders { method, path, version };
                        break;
                    }
                },

                SocketHandlerState::ExecutingHandler { response_getter, registration } => {
                    // TODO: write incoming data to request's reader
                    if let Ok(response) = response_getter.try_recv() {
                        assert!(response.upgrade.is_none());

                        let mut headers_data = Vec::new();
                        write!(headers_data, "HTTP/1.1 {} Ok\r\n", response.status_code).unwrap();
                        for (header, value) in response.headers {
                            write!(headers_data, "{}: {}\r\n", header, value).unwrap();
                        }
                        write!(headers_data, "\r\n").unwrap();

                        let (body_data, _) = response.data.into_reader_and_size();
                        let full_data = Cursor::new(headers_data).chain(body_data);

                        self.state = SocketHandlerState::SendingResponse {
                            data: Box::new(full_data)
                        };

                    } else {
                        self.state = SocketHandlerState::ExecutingHandler { response_getter, registration };
                        break;
                    }
                },

                SocketHandlerState::SendingResponse { mut data } => {
                    let old_pw_len = update.pending_write_buffer.len();
                    update.pending_write_buffer.resize(old_pw_len + 256, 0);

                    match data.read(&mut update.pending_write_buffer[old_pw_len..]) {
                        Ok(0) => {
                            update.pending_write_buffer.resize(old_pw_len, 0);
                            self.state = SocketHandlerState::WaitingForRqLine;
                            break;
                        },
                        Ok(n) => {
                            update.pending_write_buffer.resize(old_pw_len + n, 0);
                            self.state = SocketHandlerState::SendingResponse { data };
                            break;
                        },
                        Err(ref e) if e.kind() == ErrorKind::Interrupted => {
                            update.pending_write_buffer.resize(old_pw_len, 0);
                            self.state = SocketHandlerState::SendingResponse { data };
                        },
                        Err(e) => {
                            update.pending_write_buffer.resize(old_pw_len, 0);
                            panic!("{:?}", e);      // FIXME:
                        },
                    };
                },

                SocketHandlerState::Closed => {
                    debug_assert!(!update.accepts_read);
                    self.state = SocketHandlerState::Closed;
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

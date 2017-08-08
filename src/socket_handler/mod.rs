// Copyright (c) 2017 The Rouille developers
// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT
// license <LICENSE-MIT or http://opensource.org/licenses/MIT>,
// at your option. All files in the project carrying such
// notice may not be copied, modified, or distributed except
// according to those terms.

use std::net::SocketAddr;
use std::sync::Arc;
use mio::Registration;

use Request;
use Response;

use self::http1::Http1Handler;
use self::rustls::RustlsHandler;
pub use self::task_pool::TaskPool;      // TODO: shouldn't be pub, but is used by Server, move it somewher else

mod http1;
mod rustls;
mod task_pool;

/// Parses the data received by a socket and returns the data to send back.
pub struct SocketHandlerDispatch {
    inner: SocketHandlerDispatchInner,
}

enum SocketHandlerDispatchInner {
    Http(Http1Handler),
    Https(RustlsHandler<Http1Handler>),
}

impl SocketHandlerDispatch {
    /// Initialization.
    pub fn new<F>(client_addr: SocketAddr, protocol: Protocol, task_pool: TaskPool,
                  handler: F) -> SocketHandlerDispatch
        where F: FnMut(Request) -> Response + Send + 'static
    {
        let http_handler = Http1Handler::new(client_addr, protocol, task_pool, handler);

        let inner = match protocol {
            Protocol::Http => SocketHandlerDispatchInner::Http(http_handler),
            Protocol::Https => SocketHandlerDispatchInner::Https(RustlsHandler::new(http_handler)),
        };

        SocketHandlerDispatch {
            inner: inner,
        }
    }
}

impl SocketHandler for SocketHandlerDispatch {
    fn update(&mut self, update: &mut Update) -> UpdateResult {
        match self.inner {
            SocketHandlerDispatchInner::Http(ref mut http) => http.update(update),
            SocketHandlerDispatchInner::Https(ref mut https) => https.update(update),
        }
    }
}

/// Protocol that can serve HTTP.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Protocol {
    Http,
    Https,
}

pub trait SocketHandler {
    /// Call this function whenever new data is received on the socket, or when the registration
    /// wakes up.
    fn update(&mut self, update: &mut Update) -> UpdateResult;
}

#[derive(Debug)]
pub struct UpdateResult {
    /// When `Some`, means that the user must call `update` when the `Registration` becomes ready
    /// (either for reading or writing). The registration should be registered with `oneshot()`.
    pub registration: Option<(Arc<Registration>, RegistrationState)>,

    /// Set to true if the socket handler will no longer process incoming data. If
    /// `close_read` is true, `pending_write_buffer` is empty, and `registration` is empty,
    /// then you can drop the socket.
    pub close_read: bool,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum RegistrationState {
    /// It is the first time this registration is returned.
    FirstTime,
    /// This registration has been registered before, and `reregister` should be used.
    Reregister,
}

/// Represents the communication between the `SocketHandler` and the outside.
///
/// The "outside" is supposed to fill `pending_read_buffer` with incoming data, and remove data
/// from `pending_write_buffer`, then call `update`.
#[derive(Debug)]
pub struct Update {
    /// Filled by the handler user and emptied by `update()`. Contains the data that comes from
    /// the client.
    // TODO: try VecDeque and check perfs
    pub pending_read_buffer: Vec<u8>,

    /// Filled by `SocketHandler::update()` and emptied by the user. Contains the data that must
    /// be sent back to the client.
    // TODO: try VecDeque and check perfs
    pub pending_write_buffer: Vec<u8>,

}

impl Update {
    /// Builds a new empty `Update`.
    pub fn empty() -> Update {
        // TODO: don't create two Vecs for each socket
        Update {
            pending_read_buffer: Vec::with_capacity(1024),
            pending_write_buffer: Vec::with_capacity(1024),
        }
    }
}

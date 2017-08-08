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
pub use self::task_pool::TaskPool;      // TODO: shouldn't be pub, but is used by Server, move it somewher else

mod http1;
mod task_pool;

/// Parses the data received by a socket and returns the data to send back.
pub struct SocketHandler {
    inner: Http1Handler,
}

impl SocketHandler {
    /// Initialization.
    pub fn new<F>(client_addr: SocketAddr, task_pool: TaskPool, handler: F) -> SocketHandler
        where F: FnMut(Request) -> Response + Send + 'static
    {
        SocketHandler {
            inner: Http1Handler::new(client_addr, Protocol::Http /* TODO: */, task_pool, handler)
        }
    }

    /// Call this function whenever new data is received on the socket, or when the registration
    /// wakes up.
    pub fn update(&mut self, update: &mut Update) {
        self.inner.update(update)
    }
}

/// Protocol that can serve HTTP.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Protocol {
    Http,
    Https,
}

/// Represents the communication between the `SocketHandler` and the outside.
///
/// The "outside" is supposed to fill `pending_read_buffer` with incoming data, and remove data
/// from `pending_write_buffer`, then call `update`.
pub struct Update {
    /// Filled by the handler user. Contains the data that comes from the client.
    pub pending_read_buffer: Vec<u8>,

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
    /// Builds a new empty `Update`.
    pub fn empty() -> Update {
        // TODO: don't create two Vecs for each socket
        Update {
            pending_read_buffer: Vec::with_capacity(1024),
            accepts_read: true,
            pending_write_buffer: Vec::with_capacity(1024),
            registration: None,
        }
    }
}

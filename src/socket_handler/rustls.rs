// Copyright (c) 2017 The Rouille developers
// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT
// license <LICENSE-MIT or http://opensource.org/licenses/MIT>,
// at your option. All files in the project carrying such
// notice may not be copied, modified, or distributed except
// according to those terms.

use std::io::Read;
use std::io::Write;
use std::sync::Arc;

use rustls::ServerConfig;
use rustls::ServerSession;
use rustls::Session;

use socket_handler::SocketHandler;
use socket_handler::Update;

/// Handles the processing of a client connection through TLS.
pub struct RustlsHandler<H> {
    // The inner handler.
    handler: H,
    // The Rustls session.
    session: ServerSession,
    // The update object to communicate with the handler.
    handler_update: Update,
}

// TODO: not working since we don't provide any certificate or anything

impl<H> RustlsHandler<H> {
    pub fn new(inner: H) -> RustlsHandler<H> {
        let dummy_config = ServerConfig::new();

        RustlsHandler {
            handler: inner,
            session: ServerSession::new(&Arc::new(dummy_config)),
            handler_update: Update::empty(),
        }
    }
}

impl<H> SocketHandler for RustlsHandler<H>
    where H: SocketHandler
{
    fn update(&mut self, update: &mut Update) {
        let read_num = self.session.read_tls(&mut (&update.pending_read_buffer[..])).unwrap();
        assert_eq!(read_num, update.pending_read_buffer.len());
        update.pending_read_buffer.clear();

        self.session.process_new_packets().unwrap();        // TODO: propagate error

        self.session.read_to_end(&mut self.handler_update.pending_read_buffer).unwrap();

        self.handler.update(&mut self.handler_update);

        self.session.write_all(&self.handler_update.pending_write_buffer).unwrap();
        self.handler_update.pending_write_buffer.clear();

        self.session.write_tls(&mut update.pending_write_buffer).unwrap();
    }
}

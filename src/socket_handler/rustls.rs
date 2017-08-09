// Copyright (c) 2017 The Rouille developers
// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT
// license <LICENSE-MIT or http://opensource.org/licenses/MIT>,
// at your option. All files in the project carrying such
// notice may not be copied, modified, or distributed except
// according to those terms.

use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;

use rustls::ResolvesServerCert;
use rustls::ServerConfig;
use rustls::ServerSession;
use rustls::Session;
use rustls::SignatureScheme;
use rustls::internal::pemfile;
use rustls::sign::CertChainAndSigner;
use rustls::sign::RSASigner;

use socket_handler::SocketHandler;
use socket_handler::Update;
use socket_handler::UpdateResult;

#[derive(Clone)]
pub struct RustlsConfig {
    config: Arc<ServerConfig>,
    inner: RustlsConfigInner,
}

#[derive(Clone)]
struct RustlsConfigInner {
    certificates: Arc<Mutex<HashMap<String, CertChainAndSigner>>>,
}

impl ResolvesServerCert for RustlsConfigInner {
    fn resolve(&self, server_name: Option<&str>, _: &[SignatureScheme])
               -> Option<CertChainAndSigner>
    {
        let server_name = match server_name {
            Some(s) => s,
            None => return None,
        };

        let certificates = self.certificates.lock().unwrap();
        certificates
            .get(server_name)
            .map(|v| v.clone())
    }
}

impl RustlsConfig {
    pub fn new() -> RustlsConfig {
        let inner = RustlsConfigInner {
            certificates: Arc::new(Mutex::new(HashMap::new())),
        };

        let mut config = ServerConfig::new();
        config.cert_resolver = Box::new(inner.clone());
        RustlsConfig {
            config: Arc::new(config),
            inner: inner,
        }
    }

    pub fn set_certificate_from_pem<S, Pu, Pr>(&self, domain_name: S, pub_pem: Pu, priv_pem: Pr)
        where S: Into<String>,
              Pu: AsRef<Path>,
              Pr: AsRef<Path>
    {
        // TODO: better error handling
        let pub_chain = {
            let pub_file = File::open(pub_pem).expect("Failed to open public PEM file");
            let mut pub_file = BufReader::new(pub_file);
            pemfile::certs(&mut pub_file).expect("Failed to parse public PEM file")
        };

        let priv_key = {
            let priv_file = File::open(priv_pem).expect("Failed to open private PEM file");
            let mut priv_file = BufReader::new(priv_file);
            // TODO: PKCS8
            let mut keys = pemfile::rsa_private_keys(&mut priv_file).expect("Failed to parse private PEM file");
            if keys.len() != 1 {
                panic!("No private key in PEM file, or multiple keys found");
            }
            keys.remove(0)
        };

        let signer = RSASigner::new(&priv_key).expect("Failed to create RSASigner");

        let mut certificates = self.inner.certificates.lock().unwrap();
        certificates.insert(domain_name.into(), (pub_chain, Arc::new(Box::new(signer) as Box<_>)));
    }
}

/// Handles the processing of a client connection through TLS.
pub struct RustlsHandler<H> {
    // The inner handler.
    handler: H,
    // The Rustls session.
    session: ServerSession,
    // The update object to communicate with the handler.
    handler_update: Update,
}

impl<H> RustlsHandler<H> {
    pub fn new(config: RustlsConfig, inner: H) -> RustlsHandler<H> {
        RustlsHandler {
            handler: inner,
            session: ServerSession::new(&config.config),
            handler_update: Update::empty(),
        }
    }
}

impl<H> SocketHandler for RustlsHandler<H>
    where H: SocketHandler
{
    fn update(&mut self, update: &mut Update) -> UpdateResult {
        let read_num = self.session.read_tls(&mut (&update.pending_read_buffer[..])).unwrap();
        assert_eq!(read_num, update.pending_read_buffer.len());
        update.pending_read_buffer.clear();

        if let Err(_) = self.session.process_new_packets() {
            // Drop the socket.
            update.pending_write_buffer.clear();
            return UpdateResult {
                registration: None,
                close_read: true,
                write_flush_suggested: false,
            };
        }

        self.session.read_to_end(&mut self.handler_update.pending_read_buffer).unwrap();

        let result = self.handler.update(&mut self.handler_update);

        self.session.write_all(&self.handler_update.pending_write_buffer).unwrap();
        self.handler_update.pending_write_buffer.clear();

        self.session.write_tls(&mut update.pending_write_buffer).unwrap();

        result
    }
}

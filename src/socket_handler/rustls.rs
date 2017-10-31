// Copyright (c) 2017 The Rouille developers
// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT
// license <LICENSE-MIT or http://opensource.org/licenses/MIT>,
// at your option. All files in the project carrying such
// notice may not be copied, modified, or distributed except
// according to those terms.

use std::collections::HashMap;
use std::error::Error;
use std::fs::File;
use std::io::BufReader;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;

use rustls::Certificate;
use rustls::PrivateKey;
use rustls::ResolvesServerCert;
use rustls::ServerConfig;
use rustls::ServerSession;
use rustls::ServerSessionMemoryCache;
use rustls::Session;
use rustls::SignatureScheme;
use rustls::internal::pemfile;
use rustls::sign::CertChainAndSigner;
use rustls::sign::RSASigner;

use socket_handler::SocketHandler;
use socket_handler::Update;
use socket_handler::UpdateResult;

/// Configuration for HTTPS handling.
///
/// This struct internally contains `Arc`s, which means that you can clone it for a cheap cost.
///
/// Note that this configuration can be updated at runtime. Certificates can be added or removed
/// while the server is running. This will only affect new HTTP connections though.
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
    /// Builds a new configuration. You should do this at initialization only.
    ///
    /// Once the configuration is created, you should add certificates to it. Otherwise people
    /// won't be able to connect to it.
    pub fn new() -> RustlsConfig {
        let inner = RustlsConfigInner {
            certificates: Arc::new(Mutex::new(HashMap::new())),
        };

        let mut config = ServerConfig::new();
        //config.alpn_protocols = vec!["http/1.1".to_owned()];      // TODO:
        config.cert_resolver = Box::new(inner.clone());
        config.session_storage = Mutex::new(ServerSessionMemoryCache::new(1024));

        RustlsConfig {
            config: Arc::new(config),
            inner: inner,
        }
    }

    /// Removes the certificate of a domain name.
    pub fn remove_certificate<S>(&self, domain_name: &str) {
        let mut certificates = self.inner.certificates.lock().unwrap();
        certificates.remove(domain_name);
    }

    /// Sets the certificate of a domain name. The certificates and private key are parsed from
    /// PEM files whose path is passed as parameter.
    ///
    /// Replaces the existing certificate for this domain name if one has been set earlier.
    pub fn set_certificate_from_pem<S, Pu, Pr>(&self, domain_name: S, pub_pem: Pu, priv_pem: Pr)
                                               -> Result<(), Box<Error + Send + Sync>>
        where S: Into<String>,
              Pu: AsRef<Path>,
              Pr: AsRef<Path>
    {
        let pub_chain = load_certificates(pub_pem)?;
        let priv_key = load_private_key(priv_pem)?;
        let signer = RSASigner::new(&priv_key)
            .map_err(|_| String::from("Failed to create RSASigner"))?;

        let mut certificates = self.inner.certificates.lock().unwrap();
        certificates.insert(domain_name.into(), (pub_chain, Arc::new(Box::new(signer) as Box<_>)));
        Ok(())
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
    /// Starts handling a TLS connection.
    ///
    /// This struct only performs the encoding and decoding, while the actual handling is performed
    /// by `inner`.
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
        // Pass outside data to the ServerSession.
        match self.session.read_tls(&mut (&update.pending_read_buffer[..])) {
            Ok(read_num) => {
                assert_eq!(read_num, update.pending_read_buffer.len());
                update.pending_read_buffer.clear();
            },
            Err(_) => {
                return UpdateResult {
                    registration: None,
                    close_read: true,
                    write_flush_suggested: false,
                };
            },
        };

        if let Err(_) = self.session.process_new_packets() {
            // Drop the socket in case of an error.
            update.pending_write_buffer.clear();
            return UpdateResult {
                registration: None,
                close_read: true,
                write_flush_suggested: false,
            };
        }

        // Pass data from the ServerSession to the inner handler.
        if let Err(_) = self.session.read_to_end(&mut self.handler_update.pending_read_buffer) {
            return UpdateResult {
                registration: None,
                close_read: true,
                write_flush_suggested: false,
            };
        }

        // Call the inner handler.
        let result = self.handler.update(&mut self.handler_update);

        // Pass data from the inner handler to the ServerSession.
        match self.session.write_all(&self.handler_update.pending_write_buffer) {
            Ok(_) => self.handler_update.pending_write_buffer.clear(),
            Err(_) => {
                return UpdateResult {
                    registration: None,
                    close_read: true,
                    write_flush_suggested: false,
                };
            }
        };

        // Pass data from the ServerSession to the outside.
        while self.session.wants_write() {
            if let Err(_) = self.session.write_tls(&mut update.pending_write_buffer) {
                return UpdateResult {
                    registration: None,
                    close_read: true,
                    write_flush_suggested: true,
                };
            }
        }

        result
    }
}

// Load certificates chain from a PEM file.
fn load_certificates<P>(path: P) -> Result<Vec<Certificate>, Box<Error + Send + Sync>>
    where P: AsRef<Path>
{
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let certs = pemfile::certs(&mut reader)
        .map_err(|_| String::from("Certificates PEM file contains invalid keys"))?;
    Ok(certs)
}

// Load private key from a PEM file.
fn load_private_key<P>(path: P) -> Result<PrivateKey, Box<Error + Send + Sync>>
    where P: AsRef<Path>
{
    let path = path.as_ref();

    let mut rsa_keys = {
        let file = File::open(path)?;
        let mut reader = BufReader::new(file);
        pemfile::rsa_private_keys(&mut reader)
            .map_err(|_| String::from("Private key PEM file contains invalid keys"))?
    };

    let mut pkcs8_keys = {
        let file = File::open(path)?;
        let mut reader = BufReader::new(file);
        pemfile::pkcs8_private_keys(&mut reader)
            .map_err(|_| String::from("Private key PEM file contains invalid keys"))?
    };

    Ok(if !pkcs8_keys.is_empty() {
        pkcs8_keys.remove(0)
    } else {
        rsa_keys.remove(0)
    })
}

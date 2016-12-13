// Copyright (c) 2016 The Rouille developers
// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT
// license <LICENSE-MIT or http://opensource.org/licenses/MIT>,
// at your option. All files in the project carrying such
// notice may not be copied, modified, or distributed except
// according to those terms.

//! Dispatch a request to another HTTP server.
//!
//! This module provides functionnalities to dispatch a request to another server. This can be
//! used to make rouille behave as a reverse proxy.
//!
//! This function call will return immediately after the remote server has finished sending its
//! headers. The socket to the remote will be stored in the `ResponseBody` of the response.
//!
//! # Example
//!
//! You can for example dispatch to a different server depending on the host requested by the
//! client.
//!
//! ```
//! use rouille::{Request, Response};
//! use rouille::proxy;
//!
//! fn handle_request(request: &Request) -> Response {
//!     let config = match request.header("Host") {
//!         Some(ref h) if h == "domain1.com" => {
//!             proxy::ProxyConfig {
//!                 addr: "domain1.handler.localnetwork",
//!                 replace_host: None,
//!             }
//!         },
//!
//!         Some(ref h) if h == "domain2.com" => {
//!             proxy::ProxyConfig {
//!                 addr: "domain2.handler.localnetwork",
//!                 replace_host: None,
//!             }
//!         },
//!
//!         _ => return Response::empty_404()
//!     };
//!
//!     match proxy::proxy(request, config) {
//!         Ok(r) => r,
//!         Err(_) => Response::text("Bad gateway").with_status_code(500),
//!     }
//! }
//! ```

use std::borrow::Cow;
use std::error;
use std::fmt;
use std::io;
use std::io::Error as IoError;
use std::io::BufRead;
use std::io::Read;
use std::io::Write;
use std::net::TcpStream;
use std::net::ToSocketAddrs;

use Request;
use Response;
use ResponseBody;

/// Error that can happen when dispatching the request to another server.
#[derive(Debug)]
pub enum ProxyError {
    /// Can't pass through the body of the request because it was already extracted.
    BodyAlreadyExtracted,

    /// Could not read the body from the request, or could not connect to the remote server, or
    /// the connection to the remote server closed unexpectedly.
    IoError(IoError),

    /// The destination server didn't produce compliant HTTP.
    HttpParseError,
}

impl From<IoError> for ProxyError {
    fn from(err: IoError) -> ProxyError {
        ProxyError::IoError(err)
    }
}

impl error::Error for ProxyError {
    #[inline]
    fn description(&self) -> &str {
        match *self {
            ProxyError::BodyAlreadyExtracted => {
                "the body of the request was already extracted"
            },
            ProxyError::IoError(_) => {
                "could not read the body from the request, or could not connect to the remote \
                 server, or the connection to the remote server closed unexpectedly"
            },
            ProxyError::HttpParseError => {
                "the destination server didn't produce compliant HTTP"
            },
        }
    }

    #[inline]
    fn cause(&self) -> Option<&error::Error> {
        match *self {
            ProxyError::IoError(ref e) => Some(e),
            _ => None
        }
    }
}

impl fmt::Display for ProxyError {
    #[inline]
    fn fmt(&self, fmt: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(fmt, "{}", error::Error::description(self))
    }
}

/// Configuration for the reverse proxy.
#[derive(Debug, Clone)]
pub struct ProxyConfig<A> {
    /// The address to connect to. For example `example.com:80`.
    pub addr: A,
    /// If `Some`, the `Host` header will be replaced with this value.
    pub replace_host: Option<Cow<'static, str>>,
}

/// Sends the request to another HTTP server using the configuration.
///
/// > **Note**: Implementation is very hacky for the moment.
///
/// > **Note**: SSL is not supported.
// TODO: ^
pub fn proxy<A>(request: &Request, config: ProxyConfig<A>) -> Result<Response, ProxyError>
    where A: ToSocketAddrs
{
    let mut socket = try!(TcpStream::connect(config.addr));

    let mut data = match request.data() {
        Some(d) => d,
        None => return Err(ProxyError::BodyAlreadyExtracted),
    };

    try!(socket.write_all(format!("{} {} HTTP/1.1\n", request.method(), request.raw_url()).as_bytes()));
    for &(ref header, ref value) in request.headers.iter() {        // TODO: use a getter for headers
        let value = if header == "Host" {
            if let Some(ref replace) = config.replace_host {
                &**replace
            } else {
                value
            }
        } else {
            value
        };

        try!(socket.write_all(format!("{}: {}\n", header, value).as_bytes()));
    }
    try!(socket.write_all("Connection: close\n\n".as_bytes()));
    try!(io::copy(&mut data, &mut socket));

    let mut socket = io::BufReader::new(socket);

    let mut headers = Vec::new();
    let status;
    {
        let mut lines = socket.by_ref().lines();

        {
            let line = try!(match lines.next() {
                Some(l) => l,
                None => return Err(ProxyError::HttpParseError),
            });
            let mut splits = line.splitn(3, ' ');
            let _ = splits.next();
            let status_str = match splits.next() {
                Some(l) => l,
                None => return Err(ProxyError::HttpParseError),
            };
            status = match status_str.parse() {
                Ok(s) => s,
                Err(_) => return Err(ProxyError::HttpParseError),
            };
        }

        for header in lines {
            let header = try!(header);
            if header.is_empty() { break; }

            let mut splits = header.splitn(2, ':');
            let header = match splits.next() {
                Some(v) => v,
                None => return Err(ProxyError::HttpParseError),
            };
            let val = match splits.next() {
                Some(v) => v,
                None => return Err(ProxyError::HttpParseError),
            };
            let val = &val[1..];

            headers.push((header.to_owned().into(), val.to_owned().into()));
        }
    }

    Ok(Response {
        status_code: status,
        headers: headers,
        data: ResponseBody::from_reader(socket),
        upgrade: None,
    })
}

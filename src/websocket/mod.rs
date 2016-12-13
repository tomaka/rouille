// Copyright (c) 2016 The Rouille developers
// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT
// license <LICENSE-MIT or http://opensource.org/licenses/MIT>,
// at your option. All files in the project carrying such
// notice may not be copied, modified, or distributed except
// according to those terms.

//! Support for websockets.
//!
//! Using websockets is done with the following steps:
//!
//! - The websocket client (usually the browser through some Javascript) must send a request to the
//!   server to initiate the process. Examples for how to do this in Javascript are out of scope
//!   of this documentation but should be easy to find on the web.
//! - The server written with rouille must answer that request with the `start()` function defined
//!   in this module. This function returns an error if the request is not a websocket
//!   initialization request.
//! - The `start()` function also returns a `Receiver<Websocket>` object. Once that `Receiver`
//!   contains a value, the connection has been initiated.
//! - You can then use the `Websocket` object to communicate with the client through the `Read`
//!   and `Write` traits.
//!
//! # Subprotocols
//!
//! The websocket connection will produce either text or binary messages. But these messages do not
//! have a meaning per se, and must also be interpreted in some way. The way messages are
//! interpreted during a websocket connection is called a *subprotocol*.
//!
//! When you call `start()` you have to indicate which subprotocol the connection is going to use.
//! This subprotocol must match one of the subprotocols that were passed by the client during its
//! request, otherwise `start()` will return an error. It is also possible to pass `None`, in which
//! case the subprotocol is unknown to both the client and the server.
//!
//! There are usually three ways to handle subprotocols on the server-side:
//!
//! - You don't really care about subprotocols because you use websockets for your own needs. You
//!   can just pass `None` to `start()`. The connection will thus never fail unless the client
//!   decides to.
//! - Your route only handles one subprotocol. Just pass this subprotocol to `start()` and you will
//!   get an error (which you can handle for example with `try_or_400!`) if it's not supported by
//!   the client.
//! - Your route supports multiple subprotocols. This is the most complex situation as you will
//!   have to enumerate the protocols with `requested_protocols()` and choose one.
//!
//! # Example
//! 
//! ```
//! # #[macro_use] extern crate rouille;
//! use std::sync::Mutex;
//! use std::sync::mpsc::Receiver;
//!
//! use rouille::Request;
//! use rouille::RawResponse;
//! use rouille::websocket;
//! # fn main() {}
//!
//! fn handle_request(request: &Request, websockets: &Mutex<Vec<Receiver<websocket::Websocket>>>)
//!                   -> RawResponse
//! {
//!     let (response, websocket) = try_or_400!(websocket::start(request, Some("my-subprotocol")));
//!     websockets.lock().unwrap().push(websocket);
//!     response
//! }
//! ```

pub use self::websocket::Message;
pub use self::websocket::SendError;
pub use self::websocket::Websocket;

use std::ascii::AsciiExt;
use std::sync::mpsc;
use std::vec::IntoIter as VecIntoIter;
use sha1::Sha1;
use rustc_serialize::base64::Config;
use rustc_serialize::base64::Standard;
use rustc_serialize::base64::Newline;
use rustc_serialize::base64::ToBase64;
use tiny_http::HTTPVersion;

use Request;
use ResponseBody;
use RawResponse;

mod low_level;
mod websocket;

/// Error that can happen when attempting to start websocket.
#[derive(Debug)]
pub enum WebsocketError {
    /// The request does not match a websocket request.
    ///
    /// The conditions are:
    /// - The method must be `GET`.
    /// - The HTTP version must be at least 1.1.
    /// - The request must include `Host`.
    /// - The `Connection` header must include `websocket`.
    /// - The `Sec-WebSocket-Version` header must be `13`.
    /// - Must have a `Sec-WebSocket-Key` header.
    InvalidWebsocketRequest,

    /// The subprotocol passed to the function was not requested by the client.
    WrongSubprotocol,
}

/// Builds a `Response` that initiates the websocket protocol.
pub fn start(request: &Request, subprotocol: Option<&str>)
             -> Result<(RawResponse, mpsc::Receiver<Websocket>), WebsocketError>
{
    if request.method() != "GET" {
        return Err(WebsocketError::InvalidWebsocketRequest);
    }

    // TODO:
    /*if request.http_version() < &HTTPVersion(1, 1) {
        return Err(WebsocketError::InvalidWebsocketRequest);
    }*/

    match request.header("Connection") {
        Some(ref h) if h.to_ascii_lowercase().contains("upgrade") => (),
        _ => return Err(WebsocketError::InvalidWebsocketRequest),
    }

    match request.header("Upgrade") {
        Some(ref h) if h.to_ascii_lowercase().contains("websocket") => (),
        _ => return Err(WebsocketError::InvalidWebsocketRequest),
    }

    // TODO: there are some version shanigans to handle
    // see https://tools.ietf.org/html/rfc6455#section-4.4
    match request.header("Sec-WebSocket-Version") {
        Some(ref h) if h == "13" => (),
        _ => return Err(WebsocketError::InvalidWebsocketRequest),
    }

    if let Some(sp) = subprotocol {
        if !requested_protocols(request).any(|p| p == sp) {
            return Err(WebsocketError::WrongSubprotocol);
        }
    }

    let key = {
        let in_key = match request.header("Sec-WebSocket-Key") {
            Some(h) => h,
            None => return Err(WebsocketError::InvalidWebsocketRequest),
        };

        convert_key(&in_key)
    };

    let (tx, rx) = mpsc::channel();

    let response = RawResponse {
        status_code: 101,
        headers: {
            let mut headers = Vec::new();
            headers.push(("Upgrade".into(), "websocket".into()));
            if let Some(sp) = subprotocol {
                headers.push(("Sec-Websocket-Protocol".into(), sp.to_owned().into()));      // TODO: meh alloc
            }
            headers.push(("Sec-Websocket-Accept".into(), key.into()));
            headers
        },
        data: ResponseBody::empty(),
        upgrade: Some(Box::new(tx) as Box<_>),
    };

    Ok((response, rx))
}

/// Returns a list of the websocket protocols requested by the client.
///
/// # Example
///
/// ```
/// use rouille::websocket;
/// 
/// # let request: rouille::Request = return;
/// for protocol in websocket::requested_protocols(&request) {
///     // ...
/// }
/// ```
// TODO: return references to the request
pub fn requested_protocols(request: &Request) -> RequestedProtocolsIter {
    match request.header("Sec-WebSocket-Protocol") {
        None => RequestedProtocolsIter { iter: Vec::new().into_iter() },
        Some(h) => {
            let iter = h.split(',')
                        .map(|s| s.trim())
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_owned())
                        .collect::<Vec<_>>().into_iter();
            RequestedProtocolsIter { iter: iter }
        }
    }
}

/// Iterator to the list of protocols requested by the user.
pub struct RequestedProtocolsIter {
    iter: VecIntoIter<String>,
}

impl Iterator for RequestedProtocolsIter {
    type Item = String;

    #[inline]
    fn next(&mut self) -> Option<String> {
        self.iter.next()
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.iter.size_hint()
    }
}

impl ExactSizeIterator for RequestedProtocolsIter {
}

/// Turns a `Sec-WebSocket-Key` into a `Sec-WebSocket-Accept`.
fn convert_key(input: &str) -> String {
    let mut sha1 = Sha1::new();
    sha1.update(input.as_bytes());
    sha1.update(b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11");

    sha1.digest().bytes().to_base64(Config { char_set: Standard, pad: true,
                                             line_length: None, newline: Newline::LF })
}

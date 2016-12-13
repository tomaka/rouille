// Copyright (c) 2016 The Rouille developers
// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT
// license <LICENSE-MIT or http://opensource.org/licenses/MIT>,
// at your option. All files in the project carrying such
// notice may not be copied, modified, or distributed except
// according to those terms.

//! Sessions handling.
//!
//! The main feature of this module is the `session` function which handles a session. This
//! function guarantees that a single unique identifier is assigned to each client. This identifier
//! is accessible through the parameter passed to the inner closure.
//!
//! # Basic example
//!
//! Here is a basic example showing how to get a session ID.
//!
//! ```
//! use rouille::Request;
//! use rouille::Response;
//! use rouille::session;
//!
//! fn handle_request(request: &Request) -> Response {
//!     session::session(request, "SID", 3600, |session| {
//!         let id: &str = session.id();
//!
//!         // This id is unique to each client.
//!
//!         Response::text(format!("Session ID: {}", id))
//!     })
//! }
//! ```

use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use rand;
use rand::Rng;

use Request;
use Response;
use ResponseCookie;
use input;

pub fn session<F>(request: &Request, cookie_name: &str, timeout_s: u64, inner: F) -> Response
    where F: FnOnce(&Session) -> Response
{
    let mut cookie = input::get_cookies(request).into_iter();
    let cookie = cookie.find(|&(ref k, _)| k == &cookie_name);
    let cookie = cookie.map(|(_, v)| v);

    let session = if let Some(cookie) = cookie {
        Session {
            key_was_retreived: AtomicBool::new(false),
            key_was_given: true,
            key: cookie,
        }
    } else {
        Session {
            key_was_retreived: AtomicBool::new(false),
            key_was_given: false,
            key: generate_session_id(),
        }
    };

    let mut response = inner(&session);

    if session.key_was_retreived.load(Ordering::Relaxed) {       // TODO: use `get_mut()`
        // FIXME: interaction with existing cookie
        // TODO: allow setting domain
        response.cookies.push(ResponseCookie {
            name: cookie_name.to_owned().into(),    // TODO: not zero-cost
            value: session.key.into(),
            http_only: true,
            path: Some("/".into()),
            domain: None,
            max_age: Some(timeout_s),
            secure: true,
        });
    }

    response
}

/// Contains the ID of the session.
pub struct Session {
    key_was_retreived: AtomicBool,
    key_was_given: bool,
    key: String,
}

impl Session {
    /// Returns true if the client gave us a session ID.
    ///
    /// If this returns false, then we are sure that no data is available.
    #[inline]
    pub fn client_has_sid(&self) -> bool {
        self.key_was_given
    }

    /// Returns the id of the session.
    #[inline]
    pub fn id(&self) -> &str {
        self.key_was_retreived.store(true, Ordering::Relaxed);
        &self.key
    }

    /*/// Generates a new id. This modifies the value returned by `id()`.
    // TODO: implement
    #[inline]
    pub fn regenerate_id(&self) {
        unimplemented!()
    }*/
}

/// Generates a string suitable for a session ID.
///
/// The output string doesn't contain any punctuation or character such as quotes or brackets
/// that could need to be escaped.
pub fn generate_session_id() -> String {
    // 5e+114 possibilities is reasonable.
    rand::OsRng::new().expect("Failed to initialize OsRng")     // TODO: <- handle that?
                      .gen_ascii_chars()
                      .filter(|&c| (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') ||
                                   (c >= '0' && c <= '9'))
                      .take(64).collect::<String>()
}

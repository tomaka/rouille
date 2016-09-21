// Copyright (c) 2016 The Rouille developers
// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT
// license <LICENSE-MIT or http://opensource.org/licenses/MIT>,
// at your option. All files in the project carrying such
// notice may not be copied, modified, or distributed except
// according to those terms.

#![deny(unsafe_code)]

extern crate filetime;
extern crate multipart;
extern crate rand;
extern crate rustc_serialize;
extern crate time;
extern crate tiny_http;
extern crate url;

pub use assets::match_assets;
pub use log::LogEntry;
pub use input::{SessionsManager, Session, generate_session_id};
pub use response::{Response, ResponseBody};

use std::io::Read;
use std::error;
use std::fmt;
use std::net::SocketAddr;
use std::net::ToSocketAddrs;
use std::sync::Arc;
use std::thread;

pub mod cgi;
pub mod input;

mod assets;
mod find_route;
mod log;
mod response;
mod router;

/// This macro assumes that the current function returns a `Result<_, RouteError>` and takes
/// a `Result`. If the expression you pass to the macro is an error, then a
/// `RouteError::WrongInput` is returned.
///
/// # Example
///
/// ```
/// # #[macro_use] extern crate rouille;
/// # extern crate rustc_serialize;
/// # fn main() {
/// use rouille::Request;
/// use rouille::RouteError;
///
/// fn handle_something(request: &Request) -> Result<(), RouteError> {
///     #[derive(RustcDecodable)]
///     struct FormData {
///         field1: u32,
///         field2: String,
///     }
///
///     let _data: FormData = try_or_400!(rouille::input::get_post_input(request));
///     Ok(())
/// }
/// # }
/// ```
#[macro_export]
macro_rules! try_or_400 {
    ($result:expr) => (
        try!($result.map_err(|_| $crate::RouteError::WrongInput))
    );
}

/// This macro assumes that the current function returns a `Result<_, RouteError>`. If the
/// condition you pass to the macro is false, then a `RouteError::WrongInput` is returned.
///
/// # Example
///
/// ```
/// # #[macro_use] extern crate rouille;
/// # extern crate rustc_serialize;
/// # fn main() {
/// use rouille::Request;
/// use rouille::RouteError;
///
/// fn handle_something(request: &Request) -> Result<(), RouteError> {
///     #[derive(RustcDecodable)]
///     struct FormData {
///         field1: u32,
///         field2: String,
///     }
///
///     let data: FormData = try_or_400!(rouille::input::get_post_input(request));
///     assert_or_400!(data.field1 >= 2);
///     Ok(())
/// }
/// # }
/// ```
#[macro_export]
macro_rules! assert_or_400 {
    ($cond:expr) => (
        if !$cond {
            return Err($crate::RouteError::WrongInput);
        } 
    );
}

/// An error that one of your routes can return.
///
/// This is just a convenience enum and you don't need to use it in your project
/// if you don't want to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RouteError {
    /// Couldn't find a way to handle this request.
    NoRouteFound,

    /// The user input is wrong.
    WrongInput,

    /// The user must be logged in.
    LoginRequired,

    /// The user entered a wrong login or password.
    WrongLoginPassword,

    /// The user is logged in but shouldn't be there.
    NotAuthorized,
}

impl error::Error for RouteError {
    fn description(&self) -> &str {
        match self {
            &RouteError::NoRouteFound => {
                "Couldn't find a way to handle this request."
            },
            &RouteError::WrongInput => {
                "The body of the request is malformed or missing something."
            },
            &RouteError::LoginRequired => {
                "The client must be logged in before this request can be answered."
            },
            &RouteError::WrongLoginPassword => {
                "The client attempted to login but entered a wrong login or password."
            },
            &RouteError::NotAuthorized => {
                "The client is logged in but doesn't have the permission to access this resource."
            },
        }
    }
}

impl fmt::Display for RouteError {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> Result<(), fmt::Error> {
        write!(fmt, "{}", error::Error::description(self))
    }
}

/// Starts a server and uses the given requests handler.
///
/// The request handler takes a `&Request` and must return a `Response` to send to the user.
///
/// # Common mistakes
///
/// The handler must capture its environment by value and not by reference (`'static`). If you
/// use closure, don't forget to put `move` in front of the closure.
///
/// The handler must also be thread-safe (`Send` and `Sync`).
/// For example this handler isn't thread-safe:
///
/// ```ignore
/// let mut requests_counter = 0;
///
/// rouille::start_server("localhost:80", move |request| {
///     requests_counter += 1;
///
///     // rest of the handler
/// })
/// ```
///
/// Multiple requests can be processed simultaneously, therefore you can't mutably access
/// variables from the outside.
///
/// Instead you must use a `Mutex`:
///
/// ```no_run
/// use std::sync::Mutex;
/// let requests_counter = Mutex::new(0);
///
/// rouille::start_server("localhost:80", move |request| {
///     *requests_counter.lock().unwrap() += 1;
///
///     // rest of the handler
/// # panic!()
/// })
/// ```
///
/// # Panic handling
///
/// If your request handler panicks, a 500 error will automatically be sent to the client.
///
pub fn start_server<A, F>(addr: A, handler: F) -> !
                          where A: ToSocketAddrs,
                                F: Send + Sync + 'static + Fn(&Request) -> Response
{
    let server = tiny_http::Server::http(addr).unwrap();
    let handler = Arc::new(handler);

    for mut request in server.incoming_requests() {
        // we spawn a thread in order to avoid crashing the server in case of a panic
        let handler = handler.clone();
        thread::spawn(move || {
            // TODO: don't read the body in memory immediately
            let mut data = Vec::with_capacity(request.body_length().unwrap_or(0));
            request.as_reader().read_to_end(&mut data);     // TODO: handle error

            // building the `Request` object
            let rouille_request = Request {
                url: request.url().to_owned(),
                method: request.method().as_str().to_owned(),
                headers: request.headers().iter().map(|h| (h.field.to_string(), h.value.clone().into())).collect(),
                https: false,
                data: data,
                remote_addr: request.remote_addr().clone(),
            };

            // calling the handler ; this most likely takes a lot of time
            let mut rouille_response = handler(&rouille_request);

            // writing the response
            let (res_data, res_len) = rouille_response.data.into_inner();
            let mut response = tiny_http::Response::empty(rouille_response.status_code)
                                            .with_data(res_data, res_len);

            for (key, value) in rouille_response.headers {
                if let Ok(header) = tiny_http::Header::from_bytes(key, value) {
                    response.add_header(header);
                } else {
                    // TODO: ?
                }
            }

            request.respond(response);
        });
    }

    unreachable!()
}

/// Represents a request that your handler must answer to.
///
/// This can be either a real request (received by the HTTP server) or a mock object created with
/// one of the `fake_*` constructors.
pub struct Request {
    method: String,
    url: String,
    headers: Vec<(String, String)>,
    https: bool,
    data: Vec<u8>,
    remote_addr: SocketAddr,
}

impl Request {
    /// Builds a fake HTTP request to be used during tests.
    ///
    /// The remote address of the client will be `127.0.0.1:12345`. Use `fake_http_from` to
    /// specify what the client's address should be.
    pub fn fake_http<U, M>(method: M, url: U, headers: Vec<(String, String)>, data: Vec<u8>)
                           -> Request where U: Into<String>, M: Into<String>
    {
        Request {
            url: url.into(),
            method: method.into(),
            https: false,
            data: data,
            headers: headers,
            remote_addr: "127.0.0.1:12345".parse().unwrap(),
        }
    }

    /// Builds a fake HTTP request to be used during tests.
    pub fn fake_http_from<U, M>(from: SocketAddr, method: M, url: U,
                                headers: Vec<(String, String)>, data: Vec<u8>)
                                -> Request where U: Into<String>, M: Into<String>
    {
        Request {
            url: url.into(),
            method: method.into(),
            https: false,
            data: data,
            headers: headers,
            remote_addr: from,
        }
    }

    /// Builds a fake HTTPS request to be used during tests.
    ///
    /// The remote address of the client will be `127.0.0.1:12345`. Use `fake_https_from` to
    /// specify what the client's address should be.
    pub fn fake_https<U, M>(method: M, url: U, headers: Vec<(String, String)>, data: Vec<u8>)
                            -> Request where U: Into<String>, M: Into<String>
    {
        Request {
            url: url.into(),
            method: method.into(),
            https: true,
            data: data,
            headers: headers,
            remote_addr: "127.0.0.1:12345".parse().unwrap(),
        }
    }

    /// Builds a fake HTTPS request to be used during tests.
    pub fn fake_https_from<U, M>(from: SocketAddr, method: M, url: U,
                                 headers: Vec<(String, String)>, data: Vec<u8>)
                                 -> Request where U: Into<String>, M: Into<String>
    {
        Request {
            url: url.into(),
            method: method.into(),
            https: true,
            data: data,
            headers: headers,
            remote_addr: from,
        }
    }

    /// Returns `true` if the request uses HTTPS instead of HTTP.
    #[inline]
    pub fn secure(&self) -> bool {
        self.https
    }

    /// Returns the method of the request (`GET`, `POST`, etc.).
    #[inline]
    pub fn method(&self) -> &str {
        &self.method
    }

    /// Returns the raw URL requested by the client. It is not decoded and thus can contain strings
    /// such as `%20`, and the query parameters such as `?p=hello`.
    ///
    /// See also `url()`.
    #[inline]
    pub fn raw_url(&self) -> &str {
        &self.url
    }

    /// Returns the raw query string requested by the client. In other words, everything after the
    /// first `?` in the raw url.
    ///
    /// Returns the empty string if no query string.
    #[inline]
    pub fn raw_query_string(&self) -> &str {
        if let Some(pos) = self.url.bytes().position(|c| c == b'?') {
            self.url.split_at(pos + 1).1
        } else {
            ""
        }
    }

    /// Returns the URL requested by the client.
    ///
    /// Contrary to `raw_url`, special characters have been decoded.
    /// If there is any non-unicode character in the URL, it will be replaced with `U+FFFD`.
    pub fn url(&self) -> String {
        let url = self.url.as_bytes();
        let url = if let Some(pos) = url.iter().position(|&c| c == b'?') {
            &url[..pos]
        } else {
            url
        };

        url::percent_encoding::lossy_utf8_percent_decode(url)
    }

    /// Returns the value of a GET parameter.
    /// TODO: clumbsy
    pub fn get_param(&self, param_name: &str) -> Option<String> {
        let get_params = &self.raw_url()[self.raw_url().bytes().position(|c| c == b'?').unwrap_or(0) ..];

        // TODO: `hello=5` will be matched for param name `lo`

        let param = match get_params.rfind(&format!("{}=", param_name)) {
            Some(p) => p + param_name.len() + 1,
            None => return None,
        };

        let value = match get_params.bytes().skip(param).position(|c| c == b'&') {
            None => &get_params[param..],
            Some(e) => &get_params[param .. e + param],
        };

        Some(url::percent_encoding::lossy_utf8_percent_decode(value.as_bytes()))
    }

    /// Returns the value of a header of the request.
    ///
    /// Returns `None` if no such header could be found.
    #[inline]
    pub fn header(&self, key: &str) -> Option<String> {
        self.headers.iter().find(|&&(ref k, _)| k == key).map(|&(_, ref v)| v.clone())
    }

    /// UNSTABLE. Returns the body of the request.
    ///
    /// Will eventually return an object that implements `Read` instead of a `Vec<u8>`.
    pub fn data(&self) -> Vec<u8> {
        self.data.clone()
    }

    /// Returns the address of the client that made this request.
    #[inline]
    pub fn remote_addr(&self) -> &SocketAddr {
        &self.remote_addr
    }
}

// Copyright (c) 2016 The Rouille developers
// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT
// license <LICENSE-MIT or http://opensource.org/licenses/MIT>,
// at your option. All files in the project carrying such
// notice may not be copied, modified, or distributed except
// according to those terms.

//! The rouille library is very easy to get started with.
//!
//! Listening to a port is done by calling the [`start_server`](fn.start_server.html) function:
//!
//! ```no_run
//! use rouille::Request;
//! use rouille::Response;
//!
//! rouille::start_server("0.0.0.0:80", move |request| {
//!     Response::text("hello world")
//! });
//! ```
//!
//! Whenever an HTTP request is received on the address passed as first parameter, the closure
//! passed as second parameter is called. This closure must then return a
//! [`Response`](struct.Response.html) that will be sent back to the client.
//!
//! See the documentation of [`start_server`](fn.start_server.html) for more details.
//!
//! # Analyzing the request
//!
//! The parameter that the closure receives is a [`Request`](struct.Request.html) object that
//! represents the request made by the client.
//!
//! The `Request` object itself provides some getters, but most advanced functionnalities are
//! provided by other modules of this crate.
//!
//! - In order to dispatch between various code depending on the URL, you can use the `router!`
//!   macro.
//! - In order to analyze the body of the request, like handling JSON input, form input, etc. you
//!   can take a look at [the `input` module](input/index.html).
//!
//! # Returning a response
//!
//! Once you analyzed the request, it is time to return a response by returning a
//! [`Response`](struct.Response.html) object.
//!
//! All the members of `Response` are public, so you can customize it as you want. There are also
//! several constructors that you build a basic `Response` which can then modify.
//!
//! In order to serve static files, take a look at
//! [the `match_assets` function](fn.match_assets.html).

#![deny(unsafe_code)]

extern crate chrono;
extern crate filetime;
extern crate multipart;
extern crate rand;
extern crate rustc_serialize;
extern crate sha1;
extern crate time;
extern crate tiny_http;
extern crate url;

pub use assets::match_assets;
pub use log::log;
pub use response::{Response, ResponseBody};
pub use tiny_http::ReadWrite;

use std::io::Cursor;
use std::io::Result as IoResult;
use std::io::Read;
use std::marker::PhantomData;
use std::net::SocketAddr;
use std::net::ToSocketAddrs;
use std::panic;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::ascii::AsciiExt;

pub mod cgi;
pub mod input;
pub mod proxy;
pub mod session;
pub mod websocket;

mod assets;
mod find_route;
mod log;
mod response;
mod router;

/// This macro assumes that the current function returns a `Response` and takes a `Result`.
/// If the expression you pass to the macro is an error, then a 400 response is returned.
///
/// # Example
///
/// ```
/// # #[macro_use] extern crate rouille;
/// # extern crate rustc_serialize;
/// # fn main() {
/// use rouille::Request;
/// use rouille::Response;
///
/// fn handle_something(request: &Request) -> Response {
///     let data = try_or_400!(post_input!(request, {
///         field1: u32,
///         field2: String,
///     }));
///
///     Response::text("hello")
/// }
/// # }
/// ```
#[macro_export]
macro_rules! try_or_400 {
    ($result:expr) => (
        match $result {
            Ok(r) => r,
            Err(_) => return $crate::Response::empty_400(),
        }
    );
}

/// This macro assumes that the current function returns a `Response` and takes a `Result`.
/// If the expression you pass to the macro is an error, then a 404 response is returned.
#[macro_export]
macro_rules! try_or_404 {
    ($result:expr) => (
        match $result {
            Ok(r) => r,
            Err(_) => return $crate::Response::empty_404(),
        }
    );
}

/// This macro assumes that the current function returns a `Response`. If the condition you pass
/// to the macro is false, then a 400 response is returned.
///
/// # Example
///
/// ```
/// # #[macro_use] extern crate rouille;
/// # fn main() {
/// use rouille::Request;
/// use rouille::Response;
///
/// fn handle_something(request: &Request) -> Response {
///     let data = try_or_400!(post_input!(request, {
///         field1: u32,
///         field2: String,
///     }));
///
///     assert_or_400!(data.field1 >= 2);
///     Response::text("hello")
/// }
/// # }
/// ```
#[macro_export]
macro_rules! assert_or_400 {
    ($cond:expr) => (
        if !$cond {
            return $crate::Response::empty_400();
        } 
    );
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
/// ```should_fail
/// let mut requests_counter = 0;
///
/// rouille::start_server("localhost:80", move |request| {
///     requests_counter += 1;
///
///     // ... rest of the handler ...
/// # panic!()
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
    let handler = Arc::new(AssertUnwindSafe(handler));      // TODO: using AssertUnwindSafe here is wrong, but unwind safety has some usability problems in Rust in general

    for request in server.incoming_requests() {
        // We spawn a thread so that requests are processed in parallel.
        let handler = handler.clone();
        thread::spawn(move || {
            // Small helper struct that makes it possible to put
            // a `tiny_http::Request` inside a `Box<Read>`.
            struct RequestRead(Arc<Mutex<Option<tiny_http::Request>>>);
            impl Read for RequestRead {
                #[inline]
                fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
                    self.0.lock().unwrap().as_mut().unwrap().as_reader().read(buf)
                }
            }

            // Building the `Request` object.
            let tiny_http_request;
            let rouille_request = {
                let url = request.url().to_owned();
                let method = request.method().as_str().to_owned();
                let headers = request.headers().iter().map(|h| (h.field.to_string(), h.value.clone().into())).collect();
                let remote_addr = request.remote_addr().clone();

                tiny_http_request = Arc::new(Mutex::new(Some(request)));

                Request {
                    url: url,
                    method: method,
                    headers: headers,
                    https: false,
                    data: Arc::new(Mutex::new(Some(Box::new(RequestRead(tiny_http_request.clone())) as Box<_>))),
                    remote_addr: remote_addr,
                }
            };

            // Calling the handler ; this most likely takes a lot of time.
            // If the handler panics, we build a dummy response.
            let mut rouille_response = {
                // We don't use the `rouille_request` anymore after the panic, so it's ok to assert
                // it's unwind safe.
                let rouille_request = AssertUnwindSafe(rouille_request);
                let res = panic::catch_unwind(move || {
                    let rouille_request = rouille_request;
                    handler(&rouille_request)
                });

                match res {
                    Ok(r) => r,
                    Err(_) => {
                        Response::html("<h1>Internal Server Error</h1>\
                                        <p>An internal error has occurred on the server.</p>")
                            .with_status_code(500)
                    }
                }
            };

            // writing the response
            let (res_data, res_len) = rouille_response.data.into_reader_and_size();
            let mut response = tiny_http::Response::empty(rouille_response.status_code)
                                            .with_data(res_data, res_len);

            let mut upgrade_header = "".into();

            for (key, value) in rouille_response.headers {
                if key.eq_ignore_ascii_case("Content-Length") {
                    continue;
                }

                if key.eq_ignore_ascii_case("Content-Encoding") {
                    continue;
                }

                if key.eq_ignore_ascii_case("Upgrade") {
                    upgrade_header = value;
                    continue;
                }

                if let Ok(header) = tiny_http::Header::from_bytes(key.as_bytes(), value.as_bytes()) {
                    response.add_header(header);
                } else {
                    // TODO: ?
                }
            }

            if let Some(ref mut upgrade) = rouille_response.upgrade {
                let trq = tiny_http_request.lock().unwrap().take().unwrap();
                let socket = trq.upgrade(&upgrade_header, response);
                upgrade.build(socket);

            } else {
                // We don't really care if we fail to send the response to the client, as there's
                // nothing we can do anyway.
                let _ = tiny_http_request.lock().unwrap().take().unwrap().respond(response);
            }
        });
    }

    unreachable!()
}

/// Trait for objects that can take ownership of a raw connection to the client data.
///
/// The purpose of this trait is to be used with the `Connection: Upgrade` header, hence its name.
pub trait Upgrade {
    /// Initializes the object with the given socket.
    fn build(&mut self, socket: Box<ReadWrite + Send>); 
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
    data: Arc<Mutex<Option<Box<Read>>>>,
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
            data: Arc::new(Mutex::new(Some(Box::new(Cursor::new(data)) as Box<_>))),
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
            data: Arc::new(Mutex::new(Some(Box::new(Cursor::new(data)) as Box<_>))),
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
            data: Arc::new(Mutex::new(Some(Box::new(Cursor::new(data)) as Box<_>))),
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
            data: Arc::new(Mutex::new(Some(Box::new(Cursor::new(data)) as Box<_>))),
            headers: headers,
            remote_addr: from,
        }
    }

    /// If the decoded URL of the request starts with `prefix`, builds a new `Request` that is
    /// the same as the original but without that prefix.
    ///
    /// # Example
    ///
    /// ```
    /// # use rouille::Request;
    /// # use rouille::Response;
    /// fn handle(request: &Request) -> Response {
    ///     if let Some(request) = request.remove_prefix("/static") {
    ///         return rouille::match_assets(&request, "/static");
    ///     }
    ///
    ///     // ...
    ///     # panic!()
    /// }
    /// ```
    pub fn remove_prefix(&self, prefix: &str) -> Option<Request> {
        if !self.url().starts_with(prefix) {
            return None;
        }
    
        // TODO: url-encoded characters in the prefix are not implemented
        assert!(self.url.starts_with(prefix));
        Some(Request {
            method: self.method.clone(),
            url: self.url[prefix.len() ..].to_owned(),
            headers: self.headers.clone(),      // TODO: expensive
            https: self.https.clone(),
            data: self.data.clone(),
            remote_addr: self.remote_addr.clone(),
        })
    }

    /// Returns `true` if the request uses HTTPS, and `false` if it uses HTTP.
    ///
    /// # Example
    ///
    /// ```
    /// use rouille::{Request, Response};
    ///
    /// fn handle(request: &Request) -> Response {
    ///     if !request.is_secure() {
    ///         return Response::redirect_303(format!("https://example.com"));
    ///     }
    ///
    ///     // ...
    /// # panic!()
    /// }
    /// ```
    #[inline]
    pub fn is_secure(&self) -> bool {
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
    ///
    /// # Example
    ///
    /// ```
    /// use rouille::Request;
    ///
    /// let request = Request::fake_http("GET", "/hello%20world?foo=bar", vec![], vec![]);
    /// assert_eq!(request.raw_url(), "/hello%20world?foo=bar");
    /// ```
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
    /// Contrary to `raw_url`, special characters have been decoded and the query string
    /// (eg `?p=hello`) has been removed.
    ///
    /// If there is any non-unicode character in the URL, it will be replaced with `U+FFFD`.
    ///
    /// > **Note**: This function will decode the token `%2F` will be decoded as `/`. However the
    /// > official speficiations say that such a token must not count as a delimiter for URL paths.
    /// > In other words, `/hello/world` is not the same as `/hello%2Fworld`.
    ///
    /// # Example
    ///
    /// ```
    /// use rouille::Request;
    ///
    /// let request = Request::fake_http("GET", "/hello%20world?foo=bar", vec![], vec![]);
    /// assert_eq!(request.url(), "/hello world");
    /// ```
    pub fn url(&self) -> String {
        let url = self.url.as_bytes();
        let url = if let Some(pos) = url.iter().position(|&c| c == b'?') {
            &url[..pos]
        } else {
            url
        };

        url::percent_encoding::percent_decode(url).decode_utf8_lossy().into_owned()
    }

    /// Returns the value of a GET parameter.
    /// TODO: clumbsy
    pub fn get_param(&self, param_name: &str) -> Option<String> {
        let get_params = self.raw_query_string();

        // TODO: `hello=5` will be matched for param name `lo`

        let param = match get_params.rfind(&format!("{}=", param_name)) {
            Some(p) => p + param_name.len() + 1,
            None => return None,
        };

        let value = match get_params.bytes().skip(param).position(|c| c == b'&') {
            None => &get_params[param..],
            Some(e) => &get_params[param .. e + param],
        };

        Some(url::percent_encoding::percent_decode(value.replace("+", " ").as_bytes()).decode_utf8_lossy().into_owned())
    }

    /// Returns the value of a header of the request.
    ///
    /// Returns `None` if no such header could be found.
    #[inline]
    pub fn header(&self, key: &str) -> Option<&str> {
        self.headers.iter().find(|&&(ref k, _)| k.eq_ignore_ascii_case(key)).map(|&(_, ref v)| &v[..])
    }

    /// Returns the state of the `DNT` (Do Not Track) header.
    ///
    /// If the header is missing or is malformed, `None` is returned. If the header exists,
    /// `Some(true)` is returned if `DNT` is `1` and `Some(false)` is returned if `DNT` is `0`.
    ///
    /// # Example
    ///
    /// ```
    /// use rouille::{Request, Response};
    ///
    /// # fn track_user(request: &Request) {}
    /// fn handle(request: &Request) -> Response {
    ///     if !request.do_not_track().unwrap_or(false) {
    ///         track_user(&request);
    ///     }
    ///
    ///     // ...
    /// # panic!()
    /// }
    /// ```
    pub fn do_not_track(&self) -> Option<bool> {
        match self.header("DNT") {
            Some(h) if h == "1" => Some(true),
            Some(h) if h == "0" => Some(false),
            _ => None
        }
    }

    /// Returns the body of the request.
    ///
    /// The body can only be retrieved once. Returns `None` is the body has already been retreived
    /// before.
    ///
    /// # Example
    ///
    /// ```
    /// use std::io::Read;
    /// use rouille::{Request, Response, ResponseBody};
    ///
    /// fn echo(request: &Request) -> Response {
    ///     let mut data = request.data().expect("Oops, body already retrieved, problem \
    ///                                           in the server");
    ///
    ///     let mut buf = Vec::new();
    ///     match data.read_to_end(&mut buf) {
    ///         Ok(_) => (),
    ///         Err(_) => return Response::text("Failed to read body")
    ///     };
    ///
    ///     Response {
    ///         data: ResponseBody::from_data(buf),
    ///         .. Response::text("")
    ///     }
    /// }
    /// ```
    pub fn data(&self) -> Option<RequestBody> {
        let reader = self.data.lock().unwrap().take();
        reader.map(|r| RequestBody { body: r, marker: PhantomData })
    }

    /// Returns the address of the client that made this request.
    ///
    /// # Example
    ///
    /// ```
    /// use rouille::{Request, Response};
    ///
    /// fn handle(request: &Request) -> Response {
    ///     Response::text(format!("Your IP is: {:?}", request.remote_addr()))
    /// }
    /// ```
    #[inline]
    pub fn remote_addr(&self) -> &SocketAddr {
        &self.remote_addr
    }
}

/// Gives access to the body of a request.
///
/// In order to obtain this object, call `request.data()`.
pub struct RequestBody<'a> {
    body: Box<Read>,
    marker: PhantomData<&'a ()>,
}

impl<'a> Read for RequestBody<'a> {
    #[inline]
    fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
        self.body.read(buf)
    }
}

#[cfg(test)]
mod tests {
    use Request;

    #[test]
    fn header() {
        let request = Request::fake_http("GET", "/", vec![("Host".to_owned(), "localhost".to_owned())], vec![]);
        assert_eq!(request.header("Host"), Some("localhost"));
        assert_eq!(request.header("host"), Some("localhost"));
    }

    #[test]
    fn get_param() {
        let request = Request::fake_http("GET", "/?p=hello", vec![], vec![]);
        assert_eq!(request.get_param("p"), Some("hello".to_owned()));
    }

    #[test]
    fn body_twice() {
        let request = Request::fake_http("GET", "/", vec![], vec![62, 62, 62]);
        assert!(request.data().is_some());
        assert!(request.data().is_none());
    }

    #[test]
    fn url_strips_get_query() {
        let request = Request::fake_http("GET", "/?p=hello", vec![], vec![]);
        assert_eq!(request.url(), "/");
    }

    #[test]
    fn urlencode_query_string() {
        let request = Request::fake_http("GET", "/?p=hello%20world", vec![], vec![]);
        assert_eq!(request.get_param("p"), Some("hello world".to_owned()));
    }

    #[test]
    fn plus_in_query_string() {
        let request = Request::fake_http("GET", "/?p=hello+world", vec![], vec![]);
        assert_eq!(request.get_param("p"), Some("hello world".to_owned()));
    }

    #[test]
    fn encoded_plus_in_query_string() {
        let request = Request::fake_http("GET", "/?p=hello%2Bworld", vec![], vec![]);
        assert_eq!(request.get_param("p"), Some("hello+world".to_owned()));
    }

    #[test]
    fn url_encode() {
        let request = Request::fake_http("GET", "/hello%20world", vec![], vec![]);
        assert_eq!(request.url(), "/hello world");
    }

    #[test]
    fn plus_in_url() {
        let request = Request::fake_http("GET", "/hello+world", vec![], vec![]);
        assert_eq!(request.url(), "/hello+world");
    }

    #[test]
    fn dnt() {
        let request = Request::fake_http("GET", "/", vec![("DNT".to_owned(), "1".to_owned())], vec![]);
        assert_eq!(request.do_not_track(), Some(true));

        let request = Request::fake_http("GET", "/", vec![("DNT".to_owned(), "0".to_owned())], vec![]);
        assert_eq!(request.do_not_track(), Some(false));

        let request = Request::fake_http("GET", "/", vec![], vec![]);
        assert_eq!(request.do_not_track(), None);

        let request = Request::fake_http("GET", "/", vec![("DNT".to_owned(), "malformed".to_owned())], vec![]);
        assert_eq!(request.do_not_track(), None);
    }
}

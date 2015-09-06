extern crate num_cpus;
extern crate rustc_serialize;
extern crate threadpool;
extern crate time;
extern crate tiny_http;
extern crate url;

pub use assets::match_assets;
pub use log::LogEntry;

use std::net::SocketAddr;
use std::net::ToSocketAddrs;
use std::str::FromStr;
use std::sync::{Arc, Mutex};
use std::thread;
use threadpool::ThreadPool;

pub mod input;

mod assets;
mod log;
mod router;

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
    // FIXME: directly pass `addr` to the `TcpListener`
    let port = addr.to_socket_addrs().unwrap().next().unwrap().port();
    let server = tiny_http::ServerBuilder::new().with_port(port).build().unwrap();

    let handler = Arc::new(handler);
    let pool = ThreadPool::new(num_cpus::get());

    loop {
        let request = server.recv().unwrap();

        let request = Request {
            url: request.url().to_owned(),
            method: request.method().as_str().to_owned(),
            https: false,
            data: Mutex::new(None),
            inner: RequestImpl::Real(Mutex::new(Some(request))),
        };

        let handler = handler.clone();

        pool.execute(move || {
            let response = (*handler)(&request);
            match request.inner {
                RequestImpl::Real(rq) => rq.lock().unwrap().take().unwrap()
                                           .respond(response.response),
                RequestImpl::Fake { .. } => unreachable!()
            }
        });
    }
}

/// Represents a request made by the client.
pub struct Request {
    url: String,
    method: String,
    https: bool,
    data: Mutex<Option<Vec<u8>>>,
    inner: RequestImpl,
}

enum RequestImpl {
    Real(Mutex<Option<tiny_http::Request>>),     // TODO: when Mutex gets "into_inner", remove the Option
    Fake { headers: Vec<(String, String)> },
}

impl Request {
    /// Builds a fake request to be used during tests.
    pub fn fake<U, M>(https: bool, url: U, method: M, headers: Vec<(String, String)>, data: Vec<u8>)
                      -> Request where U: Into<String>, M: Into<String>
    {
        Request {
            url: url.into(),
            method: method.into(),
            https: https,
            data: Mutex::new(Some(data)),
            inner: RequestImpl::Fake { headers: headers },
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

    /// Returns the URL requested by the client. It is not decoded and thus can contain `%20` or
    /// other characters.
    #[inline]
    pub fn url(&self) -> &str {
        &self.url
    }

    /// Returns the value of a header of the request.
    #[inline]
    pub fn header(&self, key: &str) -> Option<String> {
        match self.inner {
            RequestImpl::Real(ref request) => request.lock().unwrap().as_ref().unwrap().headers()
                                                     .iter().find(|h| h.field.as_str() == key)
                                                     .map(|h| h.value.as_str().to_owned()),

            RequestImpl::Fake { ref headers } => headers.iter().find(|&&(ref k, _)| k == key)
                                                        .map(|&(_, ref v)| v.clone())
        }        
    }

    /// Returns the data of the request.
    pub fn data(&self) -> Vec<u8> {
        let mut data = self.data.lock().unwrap();

        if let Some(ref mut data) = *data {
            return data.clone();
        }

        let mut read = Vec::new();
        match self.inner {
            RequestImpl::Real(ref request) => {
                let _ = request.lock().unwrap().as_mut().unwrap()
                               .as_reader().read_to_end(&mut read);
            },
            RequestImpl::Fake { .. } => ()
        };
        let read_clone = read.clone();
        *data = Some(read);
        read_clone
    }
}

/// Contains a prototype of a response. The response is only sent when you call `Request::respond`.
pub struct Response {
    response: tiny_http::ResponseBox,
}

impl Response {
    /// UNSTABLE. Builds a default response to handle the given route error.
    ///
    /// Important: don't use this in a real website. This function is just a convenience when
    /// prototyping.
    ///
    /// For authentication-related errors, you are strongly encouraged to handle them yourself.
    #[inline]
    pub fn from_error(err: &RouteError) -> Response {
        let response = match err {
            &RouteError::NoRouteFound => tiny_http::Response::empty(404),
            &RouteError::WrongInput => tiny_http::Response::empty(400),
            &RouteError::LoginRequired => tiny_http::Response::empty(401),     // TODO: www-auth header?
            &RouteError::WrongLoginPassword => tiny_http::Response::empty(401),     // TODO: www-auth header?
            &RouteError::NotAuthorized => tiny_http::Response::empty(403),
        };

        Response { response: response.boxed() }
    }

    /// Builds a `Response` that redirects the user to another URL.
    #[inline]
    pub fn redirect(target: &str) -> Response {
        let response = tiny_http::Response::empty(303);
        // TODO: slow \|/
        let response = response.with_header(tiny_http::Header::from_str(&format!("Location: {}", target)).unwrap());

        Response { response: response.boxed() }
    }

    /// Builds a `Response` that outputs HTML.
    #[inline]
    pub fn html(content: &[u8]) -> Response {
        let response = tiny_http::Response::from_data(content);
        // TODO: slow \|/
        let response = response.with_header(tiny_http::Header::from_str("Content-Type: text/html; charset=utf8").unwrap());

        Response { response: response.boxed() }
    }

    /// Builds a `Response` that outputs plain text.
    #[inline]
    pub fn text<S>(text: S) -> Response where S: Into<String> {
        let response = tiny_http::Response::from_string(text.into());
        // TODO: slow \|/
        let response = response.with_header(tiny_http::Header::from_str("Content-Type: text/plain; charset=utf8").unwrap());

        Response { response: response.boxed() }
    }

    /// Builds a `Response` that outputs JSON.
    #[inline]
    pub fn json<T>(content: &T) -> Response where T: rustc_serialize::Encodable {
        let data = rustc_serialize::json::encode(content).unwrap();
        let response = tiny_http::Response::from_string(data);
        // TODO: slow \|/
        let response = response.with_header(tiny_http::Header::from_str("Content-Type: application/json").unwrap());

        Response { response: response.boxed() }
    }

    /// Builds a `Response` that returns a `401 Not Authorized` status
    /// and a `WWW-Authenticate` header.
    #[inline]
    pub fn basic_http_auth_login_required(realm: &str) -> Response {
        // TODO: escape the realm
        let response = tiny_http::Response::empty(401);
        // TODO: slow \|/
        let response = response.with_header(tiny_http::Header::from_str(&format!("WWW-Authenticate: Basic realm=\"{}\"", realm)).unwrap());

        Response { response: response.boxed() }   
    }
}

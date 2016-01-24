// Copyright (c) 2016 The Rouille developers
// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT
// license <LICENSE-MIT or http://opensource.org/licenses/MIT>,
// at your option. All files in the project carrying such
// notice may not be copied, modified, or distributed except
// according to those terms.

use std::io;
use std::io::Cursor;
use std::io::Read;
use std::fs::File;
use rustc_serialize;

use RouteError;

/// Contains a prototype of a response.
/// The response is only sent when you call `Request::respond`.
pub struct Response {
    /// The status code to return to the user.
    pub status_code: u16,

    /// List of headers to be returned in the response.
    ///
    /// Note that important headers such as `Connection` or `Content-Length` will be ignored
    /// from this list.
    // TODO: document precisely which headers
    pub headers: Vec<(String, String)>,

    /// An opaque type that contains the body of the response.
    pub data: ResponseBody,
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
        match err {
            &RouteError::NoRouteFound => Response {
                status_code: 404, headers: vec![], data: ResponseBody::empty()
            },
            &RouteError::WrongInput => Response {
                status_code: 400, headers: vec![], data: ResponseBody::empty()
            },
            &RouteError::LoginRequired => Response {
                status_code: 401, headers: vec![], data: ResponseBody::empty()
            },     // TODO: www-auth header?
            &RouteError::WrongLoginPassword => Response {
                status_code: 401, headers: vec![], data: ResponseBody::empty()
            },     // TODO: www-auth header?
            &RouteError::NotAuthorized => Response {
                status_code: 403, headers: vec![], data: ResponseBody::empty()
            },
        }
    }

    /// Builds a `Response` that redirects the user to another URL.
    #[inline]
    pub fn redirect(target: &str) -> Response {
        Response {
            status_code: 303,
            headers: vec![("Location".to_owned(), target.to_owned())],
            data: ResponseBody::empty(),
        }
    }

    /// Builds a `Response` that outputs HTML.
    #[inline]
    pub fn html<D>(content: D) -> Response where D: Into<Vec<u8>> {
        Response {
            status_code: 200,
            headers: vec![("Content-Type".to_owned(), "text/html; charset=utf8".to_owned())],
            data: ResponseBody::from_data(content),
        }
    }

    /// Builds a `Response` that outputs plain text.
    #[inline]
    pub fn text<S>(text: S) -> Response where S: Into<String> {
        Response {
            status_code: 200,
            headers: vec![("Content-Type".to_owned(), "text/plain; charset=utf8".to_owned())],
            data: ResponseBody::from_string(text),
        }
    }

    /// Builds a `Response` that outputs JSON.
    #[inline]
    pub fn json<T>(content: &T) -> Response where T: rustc_serialize::Encodable {
        let data = rustc_serialize::json::encode(content).unwrap();

        Response {
            status_code: 200,
            headers: vec![("Content-Type".to_owned(), "application/json".to_owned())],
            data: ResponseBody::from_data(data),
        }
    }

    /// Builds a `Response` that returns a `401 Not Authorized` status
    /// and a `WWW-Authenticate` header.
    #[inline]
    pub fn basic_http_auth_login_required(realm: &str) -> Response {
        // TODO: escape the realm
        Response {
            status_code: 401,
            headers: vec![("WWW-Authenticate".to_owned(), format!("Basic realm=\"{}\"", realm))],
            data: ResponseBody::empty(),
        }
    }

    /// Changes the status code of the response.
    #[inline]
    pub fn with_status_code(mut self, code: u16) -> Response {
        self.status_code = code;
        self
    }
}

/// An opaque type that represents the body of a response.
pub struct ResponseBody {
    data: Box<Read + Send>,
    data_length: Option<usize>,
}

impl ResponseBody {
    /// UNSTABLE. Extracts the content of the response.
    #[doc(hidden)]
    #[inline]
    pub fn into_inner(self) -> (Box<Read + Send>, Option<usize>) {
        (self.data, self.data_length)
    }

    /// Builds a `ResponseBody` that doesn't return any data.
    #[inline]
    pub fn empty() -> ResponseBody {
        ResponseBody {
            data: Box::new(io::empty()),
            data_length: Some(0),
        }
    }

    /// Builds a new `ResponseBody` that will read the data from a `Read`.
    ///
    /// Note that this is suboptimal compared to other constructors because the length
    /// isn't known in advance.
    #[inline]
    pub fn from_reader<R>(data: R) -> ResponseBody where R: Read + Send + 'static {
        ResponseBody {
            data: Box::new(data),
            data_length: None,
        }
    }

    /// Builds a new `ResponseBody` that returns the given data.
    #[inline]
    pub fn from_data<D>(data: D) -> ResponseBody where D: Into<Vec<u8>> {
        let data = data.into();
        let len = data.len();

        ResponseBody {
            data: Box::new(Cursor::new(data)),
            data_length: Some(len),
        }
    }

    /// Builds a new `ResponseBody` that returns the content of the given file.
    #[inline]
    pub fn from_file(file: File) -> ResponseBody {
        let len = file.metadata().map(|metadata| metadata.len() as usize).ok();

        ResponseBody {
            data: Box::new(file),
            data_length: len,
        }
    }

    /// Builds a new `ResponseBody` that returns an UTF-8 string.
    #[inline]
    pub fn from_string<S>(data: S) -> ResponseBody where S: Into<String> {
        ResponseBody::from_data(data.into().into_bytes())
    }
}

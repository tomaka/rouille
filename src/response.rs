// Copyright (c) 2016 The Rouille developers
// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT
// license <LICENSE-MIT or http://opensource.org/licenses/MIT>,
// at your option. All files in the project carrying such
// notice may not be copied, modified, or distributed except
// according to those terms.

use std::borrow::Cow;
use std::io;
use std::io::Cursor;
use std::io::Read;
use std::fs::File;
use rustc_serialize;
use Upgrade;

/// Contains a prototype of a response.
///
/// The response is only sent to the client when you return the `Response` object from your
/// request handler. This means that you are free to create as many `Response` objects as you want.
pub struct Response {
    /// The status code to return to the user.
    pub status_code: u16,

    /// List of headers to be returned in the response.
    ///
    /// The value of the following headers will be ignored from this list, even if present:
    ///
    /// - Accept-Ranges
    /// - Connection
    /// - Content-Encoding
    /// - Content-Length
    /// - Content-Range
    /// - Trailer
    /// - Transfer-Encoding
    ///
    /// Additionnaly, the `Upgrade` header is ignored as well unless the `upgrade` field of the
    /// `Response` is set to something.
    ///
    /// The reason for this is that these headers are too low-level and are directly handled by
    /// the underlying HTTP response system.
    ///
    /// The value of `Content-Length` is automatically determined by the `ResponseBody` object of
    /// the `data` member.
    ///
    /// If you want to send back `Connection: upgrade`, you should set the value of the `upgrade`
    /// field to something.
    pub headers: Vec<(Cow<'static, str>, Cow<'static, str>)>,

    /// An opaque type that contains the body of the response.
    pub data: ResponseBody,

    /// If set, rouille will give ownership of the client socket to the `Upgrade` object.
    ///
    /// In all circumstances, the value of the `Connection` header is managed by the framework and
    /// cannot be customized. If this value is set, the response will automatically contain
    /// `Connection: Upgrade`.
    pub upgrade: Option<Box<Upgrade + Send>>,
}

impl Response {
    /// Returns true if the status code of this `Response` indicates success.
    ///
    /// This is the range [200-399].
    ///
    /// # Example
    ///
    /// ```
    /// use rouille::Response;
    /// let response = Response::text("hello world");
    /// assert!(response.is_success());
    /// ```
    #[inline]
    pub fn is_success(&self) -> bool {
        self.status_code >= 200 && self.status_code < 400
    }

    /// Shortcut for `!response.is_success()`.
    ///
    /// # Example
    ///
    /// ```
    /// use rouille::Response;
    /// let response = Response::empty_400();
    /// assert!(response.is_error());
    /// ```
    #[inline]
    pub fn is_error(&self) -> bool {
        !self.is_success()
    }

    /// Builds a `Response` that redirects the user to another URL with a 303 status code.
    ///
    /// # Example
    ///
    /// ```
    /// use rouille::Response;
    /// let response = Response::redirect("/foo");
    /// ```
    ///
    /// Another example with a `String`:
    ///
    /// ```
    /// use rouille::Response;
    /// let user_id = 5;
    /// let response = Response::redirect(format!("/users/{}", user_id));
    /// ```
    #[inline]
    pub fn redirect<S>(target: S) -> Response
        where S: Into<Cow<'static, str>>
    {
        Response {
            status_code: 303,
            headers: vec![("Location".into(), target.into())],
            data: ResponseBody::empty(),
            upgrade: None,
        }
    }

    /// Builds a `Response` that outputs HTML.
    ///
    /// # Example
    ///
    /// ```
    /// use rouille::Response;
    /// let response = Response::text("<p>hello <strong>world</strong></p>");
    /// ```
    #[inline]
    pub fn html<D>(content: D) -> Response where D: Into<String> {
        Response {
            status_code: 200,
            headers: vec![("Content-Type".into(), "text/html; charset=utf8".into())],
            data: ResponseBody::from_string(content),
            upgrade: None,
        }
    }

    /// Builds a `Response` that outputs SVG.
    ///
    /// # Example
    ///
    /// ```
    /// use rouille::Response;
    /// let response = Response::svg("<svg xmlns='http://www.w3.org/2000/svg'/>");
    /// ```
    #[inline]
    pub fn svg<D>(content: D) -> Response where D: Into<String> {
        Response {
            status_code: 200,
            headers: vec![("Content-Type".into(), "image/svg+xml; charset=utf8".into())],
            data: ResponseBody::from_string(content),
            upgrade: None,
        }
    }

    /// Builds a `Response` that outputs plain text.
    ///
    /// # Example
    ///
    /// ```
    /// use rouille::Response;
    /// let response = Response::text("hello world");
    /// ```
    #[inline]
    pub fn text<S>(text: S) -> Response where S: Into<String> {
        Response {
            status_code: 200,
            headers: vec![("Content-Type".into(), "text/plain; charset=utf8".into())],
            data: ResponseBody::from_string(text),
            upgrade: None,
        }
    }

    /// Builds a `Response` that outputs JSON.
    ///
    /// # Example
    ///
    /// ```
    /// extern crate rustc_serialize;
    /// # #[macro_use] extern crate rouille;
    /// use rouille::Response;
    /// # fn main() {
    ///
    /// #[derive(RustcEncodable)]
    /// struct MyStruct {
    ///     field1: String,
    ///     field2: i32,
    /// }
    ///
    /// let response = Response::json(&MyStruct { field1: "hello".to_owned(), field2: 5 });
    /// // The Response will contain something like `{ field1: "hello", field2: 5 }`
    /// # }
    /// ```
    #[inline]
    pub fn json<T>(content: &T) -> Response where T: rustc_serialize::Encodable {
        let data = rustc_serialize::json::encode(content).unwrap();

        Response {
            status_code: 200,
            headers: vec![("Content-Type".into(), "application/json".into())],
            data: ResponseBody::from_data(data),
            upgrade: None,
        }
    }

    /// Builds a `Response` that returns a `401 Not Authorized` status
    /// and a `WWW-Authenticate` header.
    ///
    /// # Example
    ///
    /// ```
    /// use rouille::Response;
    /// let response = Response::basic_http_auth_login_required("realm");
    /// ```
    #[inline]
    pub fn basic_http_auth_login_required(realm: &str) -> Response {
        // TODO: escape the realm
        Response {
            status_code: 401,
            headers: vec![("WWW-Authenticate".into(), format!("Basic realm=\"{}\"", realm).into())],
            data: ResponseBody::empty(),
            upgrade: None,
        }
    }

    /// Builds an empty `Response` with a 400 status code.
    ///
    /// # Example
    ///
    /// ```
    /// use rouille::Response;
    /// let response = Response::empty_400();
    /// ```
    #[inline]
    pub fn empty_400() -> Response {
        Response {
            status_code: 400,
            headers: vec![],
            data: ResponseBody::empty(),
            upgrade: None,
        }
    }

    /// Builds an empty `Response` with a 404 status code.
    ///
    /// # Example
    ///
    /// ```
    /// use rouille::Response;
    /// let response = Response::empty_404();
    /// ```
    #[inline]
    pub fn empty_404() -> Response {
        Response {
            status_code: 404,
            headers: vec![],
            data: ResponseBody::empty(),
            upgrade: None,
        }
    }

    /// Changes the status code of the response.
    ///
    /// # Example
    ///
    /// ```
    /// use rouille::Response;
    /// let response = Response::text("hello world").with_status_code(500);
    /// ```
    #[inline]
    pub fn with_status_code(mut self, code: u16) -> Response {
        self.status_code = code;
        self
    }
}

/// An opaque type that represents the body of a response.
///
/// You can't access the inside of this struct, but you can build one by using one of the provided
/// constructors. 
///
/// # Example
///
/// ```
/// use rouille::ResponseBody;
/// let body = ResponseBody::from_string("hello world");
/// ```
pub struct ResponseBody {
    data: Box<Read + Send>,
    data_length: Option<usize>,
}

impl ResponseBody {
    /// UNSTABLE. Extracts the content of the response. Do not use.
    #[doc(hidden)]
    #[inline]
    pub fn into_inner(self) -> (Box<Read + Send>, Option<usize>) {
        (self.data, self.data_length)
    }

    /// Builds a `ResponseBody` that doesn't return any data.
    ///
    /// # Example
    ///
    /// ```
    /// use rouille::ResponseBody;
    /// let body = ResponseBody::empty();
    /// ```
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
    ///
    /// # Example
    ///
    /// ```no_run
    /// use std::io;
    /// use std::io::Read;
    /// use rouille::ResponseBody;
    ///
    /// let body = ResponseBody::from_reader(io::stdin().take(128));
    /// ```
    #[inline]
    pub fn from_reader<R>(data: R) -> ResponseBody where R: Read + Send + 'static {
        ResponseBody {
            data: Box::new(data),
            data_length: None,
        }
    }

    /// Builds a new `ResponseBody` that returns the given data.
    ///
    /// # Example
    ///
    /// ```
    /// use rouille::ResponseBody;
    /// let body = ResponseBody::from_data(vec![12u8, 97, 34]);
    /// ```
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
    ///
    /// # Example
    ///
    /// ```no_run
    /// use std::fs::File;
    /// use rouille::ResponseBody;
    ///
    /// let file = File::open("page.html").unwrap();
    /// let body = ResponseBody::from_file(file);
    /// ```
    #[inline]
    pub fn from_file(file: File) -> ResponseBody {
        let len = file.metadata().map(|metadata| metadata.len() as usize).ok();

        ResponseBody {
            data: Box::new(file),
            data_length: len,
        }
    }

    /// Builds a new `ResponseBody` that returns an UTF-8 string.
    ///
    /// # Example
    ///
    /// ```
    /// use rouille::ResponseBody;
    /// let body = ResponseBody::from_string("hello world");
    /// ```
    #[inline]
    pub fn from_string<S>(data: S) -> ResponseBody where S: Into<String> {
        ResponseBody::from_data(data.into().into_bytes())
    }
}

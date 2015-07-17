//! Client-side integration with [Hyper](https://github.com/hyperium/hyper). 
//! Enabled with the `hyper` feature (on by default).
//!
//! Contains `impl HttpRequest for Request<Fresh>` and `impl HttpStream for Request<Streaming>`.
use hyper::client::request::Request;
use hyper::client::response::Response;
use hyper::error::Error as HyperError;
use hyper::header::{ContentType, ContentLength};
use hyper::method::Method;
use hyper::net::{Fresh, Streaming};

use mime::{Mime, TopLevel, SubLevel, Attr, Value};

use super::{HttpRequest, HttpStream};

impl HttpRequest for Request<Fresh> {
    type Stream = Request<Streaming>;
    type Error = HyperError;

    /// #Panics
    /// If `self.method() != Method::Post`.
    fn apply_headers(&mut self, boundary: &str, content_len: Option<u64>) -> bool {
        if self.method() != Method::Post {
            error!(
                "Expected Hyper request method to be `Post`, was actually `{:?}`",
                self.method()
            );

            return false;
        }

        let headers = self.headers_mut();

        headers.set(ContentType(multipart_mime(boundary)));

        if let Some(size) = content_len {
            headers.set(ContentLength(size));   
        }

        debug!("Hyper headers: {}", headers); 

        true
    }

    fn open_stream(self) -> Result<Self::Stream, Self::Error> {
        self.start()
    }
} 

impl HttpStream for Request<Streaming> {
    type Request = Request<Fresh>;
    type Response = Response;
    type Error = HyperError;

    fn finish(self) -> Result<Self::Response, Self::Error> {
        self.send()
    }
}

fn multipart_mime(bound: &str) -> Mime {
    Mime(
        TopLevel::Multipart, SubLevel::Ext("form-data".into()),
        vec![(Attr::Ext("boundary".into()), Value::Ext(bound.into()))]
    )         
}


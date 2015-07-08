use hyper::client::request::Request;
use hyper::client::response::Response;
use hyper::error::Error as HyperError;
use hyper::header::{ContentType, ContentLength};
use hyper::method::Method;
use hyper::net::{Fresh, Streaming};

use super::{HttpRequest, HttpStream};

use std::io;
use std::io::prelude::*;

impl HttpRequest for Request<Fresh> {
    type Stream = Request<Streaming>;
    type Response = Response;
    type Error = HyperError;

    /// #Panics
    /// If the `Request<Fresh>` method is not `Method::Post`.
    fn apply_headers(&mut self, boundary: &str, content_len: Option<usize>) {
        assert!(self.method() == Method::Post, "Multipart request must use POST method!");        

        let headers = self.headers_mut();

        headers.set(ContentType(super::multipart_mime(boundary)));

        if let Some(size) = content_len {
            headers.set(ContentLength(size));   
        }

        debug!("Hyper headers: {}", self.headers());        
    }

    fn send<F>(self, send_fn: F) -> Self::RequestResult 
    where F: FnOnce(&mut Request<Streaming>) -> io::Result<()> {
        let mut req = try!(self.start());
        try!(send_fn(&mut req));
        req.send()
    }
} 

impl HttpStream for Request<Streaming> {
    
}

fn multipart_mime(bound: &str) -> Mime {
    Mime(
        TopLevel::Multipart, SubLevel::Ext("form-data".into()),
        vec![(Attr::Ext("boundary".into()), Value::Ext(bound.into()))]
    )         
}


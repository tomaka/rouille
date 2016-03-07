use tiny_http::Request;

use super::HttpRequest;

use std::io::Read;

impl<'r> HttpRequest for &'r mut Request {
    type Body = &'r mut Read;
    
    fn multipart_boundary(&self) -> Option<&str> {
        self.headers.iter().find(|header| 
    }
}

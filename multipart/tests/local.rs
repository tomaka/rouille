#[macro_use]
extern crate log;
extern crate env_logger;

extern crate multipart;

use multipart::client::HttpRequest as ClientRequest;
use multipart::client::HttpStream as ClientStream;

use multipart::server::HttpRequest as ServerRequest;

use std::io;
use std::io::prelude::*;

#[test]
fn local_test() {
    let buf = test_client();
    test_server(buf);
}

fn test_client() -> HttpBuffer {
    use multipart::client::Multipart;

    let request = MockClientRequest::default();
    
    Multipart::from_request(request).unwrap()
        .write_text("hello", "world")
        .write_text("goodnight", "sun")
        .send().unwrap()
}

fn test_server(buf: HttpBuffer) {
    use multipart::server::Multipart;

    let mut multipart = Multipart::from_request(buf.for_server())
        .unwrap_or_else(|_| panic!("Buffer should be multipart!"));

    while let Ok(Some(field)) = multipart.read_entry() {
        match &*field.name {
            "hello" => assert_eq!(field.data.as_text(), Some("world")),
            "goodnight" => assert_eq!(field.data.as_text(), Some("sun")),
            _ => panic!("Unexpected field: {:?}", field),
        }
    }
}

#[derive(Default, Debug)]
struct MockClientRequest {
    boundary: Option<String>,
    content_len: Option<usize>,
}

impl ClientRequest for MockClientRequest {
    type Stream = HttpBuffer;
    type Error = io::Error;
    
    fn apply_headers(&mut self, boundary: &str, content_len: Option<usize>) {
        self.boundary = Some(boundary.into());
        self.content_len = content_len;
    }

    fn open_stream(self) -> Result<HttpBuffer, io::Error> {
        debug!("MockClientRequest::open_stream called! {:?}", self);
        let boundary = self.boundary.expect("HttpRequest::set_headers() was not called!");
        
        Ok(HttpBuffer { buf: Vec::new(), boundary: boundary, content_len: self.content_len })
    }
}

#[derive(Debug)]
struct HttpBuffer {
    buf: Vec<u8>,
    boundary: String,
    content_len: Option<usize>,
}

impl HttpBuffer {
    fn for_server(&self) -> ServerBuffer {
        ServerBuffer {
            data: &self.buf,
            boundary: &self.boundary,
            content_len: self.content_len,
        }
    }
}

impl Write for HttpBuffer {
    fn write(&mut self, data: &[u8]) -> io::Result<usize> {
        self.buf.write(data)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.buf.flush()
    }
}

impl Read for HttpBuffer {
    fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
        unimplemented!()
    }
}

impl ClientStream for HttpBuffer {
    type Request = MockClientRequest;
    type Response = HttpBuffer;
    type Error = io::Error;

    fn finish(self) -> Result<Self, io::Error> { Ok(self) }
}

#[derive(Debug)]
struct ServerBuffer<'a> {
    data: &'a [u8],
    boundary: &'a str,
    content_len: Option<usize>,
}

impl<'a> Read for ServerBuffer<'a> {
    fn read(&mut self, out: &mut [u8]) -> io::Result<usize> {
        self.data.read(out)
    }
}

impl<'a> ServerRequest for ServerBuffer<'a> {
    fn is_multipart(&self) -> bool { true }
    fn boundary(&self) -> Option<&str> { Some(&self.boundary) }
}


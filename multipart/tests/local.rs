#[macro_use]
extern crate log;
extern crate env_logger;
extern crate rand;

extern crate multipart;

use multipart::client::HttpRequest as ClientRequest;
use multipart::client::HttpStream as ClientStream;

use multipart::server::HttpRequest as ServerRequest;

use rand::Rng;
use rand::distributions::{Range, Sample};

use std::collections::HashMap;
use std::io;
use std::io::prelude::*;

struct TestFields {
    texts: HashMap<String, String>,
    files: HashMap<String, Vec<u8>>,
}

#[test]
fn local_test() {
    let test_fields = gen_test_fields();

    let buf = test_client(&test_fields);
    test_server(buf, test_fields);
}

fn gen_test_fields() -> TestFields {
    const MIN_FIELDS: usize = 1;
    const MAX_FIELDS: usize = 5;

    let texts_count = gen_range(MIN_FIELDS, MAX_FIELDS);
    let files_count = gen_range(MIN_FIELDS, MAX_FIELDS);

    TestFields {
        texts: (0..texts_count).map(|_| (gen_string(), gen_string())).collect(),
        files: (0..files_count).map(|_| (gen_string(), gen_bytes())).collect(),
    }
}

fn gen_range(min: usize, max: usize) -> usize {
    Range::new(min, max).sample(&mut rand::weak_rng())
}

fn gen_string() -> String {
    const MIN_LEN: usize = 3;
    const MAX_LEN: usize = 12;

    let mut rng = rand::weak_rng();
    let str_len = gen_range(MIN_LEN, MAX_LEN);

    rng.gen_ascii_chars().take(str_len).collect()
}

fn gen_bytes() -> Vec<u8> {
    const MIN_LEN: usize = 64;
    const MAX_LEN: usize = 1024;

    let mut rng = rand::weak_rng();
    let bytes_len = gen_range(MIN_LEN, MAX_LEN);

    let mut vec = vec![0u8; bytes_len];
    rng.fill_bytes(&mut vec);
    vec
}


fn test_client(test_fields: &TestFields) -> HttpBuffer {
    use multipart::client::Multipart;

    let request = MockClientRequest::default();

    let mut test_files = test_fields.files.iter();

    let mut multipart = Multipart::from_request(request).unwrap();
    
    for (name, text) in &test_fields.texts {
        if let Some((file_name, file)) = test_files.next() {
            multipart = multipart.write_stream(file_name, &mut &**file, None, None);
        }

        multipart = multipart.write_text(name, text);    
    }

    multipart.send().unwrap()
}

fn test_server(buf: HttpBuffer, mut fields: TestFields) {
    use multipart::server::{Multipart, MultipartData};

    let mut multipart = Multipart::from_request(buf.for_server())
        .unwrap_or_else(|_| panic!("Buffer should be multipart!"));

    while let Ok(Some(mut field)) = multipart.read_entry() {
        match field.data {
            MultipartData::Text(text) => {
                let test_text = fields.texts.remove(&field.name).unwrap();
                assert!(
                    text == test_text, 
                    "Expected {:?} for {:?} got {:?}", 
                    text, field.name, test_text
                );

            },
            MultipartData::File(ref mut file) => {
                let test_bytes = fields.files.remove(&field.name).unwrap();

                let mut bytes = Vec::with_capacity(test_bytes.len());
                file.read_to_end(&mut bytes).unwrap();

                assert!(bytes == test_bytes, "Unexpected data for {:?}", field.name);
            },
        }
    }

    assert!(fields.texts.is_empty(), "Text fields were not exhausted! Text fields: {:?}", fields.texts);
    assert!(fields.files.is_empty(), "File fields were not exhausted!");
}

#[derive(Default, Debug)]
pub struct MockClientRequest {
    boundary: Option<String>,
    content_len: Option<u64>,
}

impl MockClientRequest {
    pub fn new() -> MockClientRequest {
        Self::default()
    }
}

impl ClientRequest for MockClientRequest {
    type Stream = HttpBuffer;
    type Error = io::Error;
    
    fn apply_headers(&mut self, boundary: &str, content_len: Option<u64>) -> bool {
        self.boundary = Some(boundary.into());
        self.content_len = content_len;
        true
    }

    fn open_stream(self) -> Result<HttpBuffer, io::Error> {
        debug!("MockClientRequest::open_stream called! {:?}", self);
        let boundary = self.boundary.expect("HttpRequest::set_headers() was not called!");
        
        Ok(HttpBuffer { buf: Vec::new(), boundary: boundary, content_len: self.content_len })
    }
}

#[derive(Debug)]
pub struct HttpBuffer {
    buf: Vec<u8>,
    boundary: String,
    content_len: Option<u64>,
}

impl HttpBuffer {
    pub fn for_server(&self) -> ServerBuffer {
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
pub struct ServerBuffer<'a> {
    data: &'a [u8],
    boundary: &'a str,
    content_len: Option<u64>,
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


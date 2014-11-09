#![feature(if_let, slicing_syntax, default_type_params, phase, macro_rules)]
extern crate hyper;
#[phase(plugin, link)] extern crate log;

extern crate mime;
extern crate serialize;

use self::mime::Mime;

use std::io::fs::File;
use std::io::{RefReader, IoResult};

use server::BoundaryReader;

pub mod client;
pub mod server;

pub struct MultipartFile {
    filename: Option<String>,
    reader: FileEntryReader,
    content_type: Mime,       
}

pub enum MultipartField {
    TextField(String),
    FileField(MultipartFile),
    // MultiFiles(Vec<MultipartFile>), /* TODO: Multiple files */
}

pub enum FileEntryReader {
    FileStream(File),
    OctetStream(BoundaryReader),
}

impl Reader for FileEntryReader {
    fn read(&mut self, buf: &mut [u8]) -> IoResult<uint>{
        match *self {
            FileStream(ref mut rdr) => rdr.read(buf),
            OctetStream(ref mut rdr) => rdr.read(buf),   
        }
    }
}

#[cfg(test)]
mod test {
   use hyper::Url;
   use hyper::client::request::Request as ClientReq;
   use client::Multipart as ClientMulti;

    #[test]
    fn client_api_test() {        
        let request = ClientReq::get(Url::parse("http://localhost:1337/").unwrap()).unwrap();

        let mut multipart = ClientMulti::new();

        multipart.add_text("hello", "world");
        multipart.add_text("goodnight", "sun");

        multipart.send(request);        
    }
       
}

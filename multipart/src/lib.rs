#![feature(if_let, slicing_syntax, default_type_params, phase)]
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

pub struct MultipartFile<'a> {
    filename: Option<String>,
    reader: FileEntryReader<'a>,
    content_type: Mime,       
}

pub enum MultipartField<'a> {
    TextField(&'a str),
    FileField(MultipartFile<'a>),
    MultiFiles(Vec<MultipartFile<'a>>),
}

pub enum FileEntryReader<'a> {
    FileStream(&'a mut File),
    OctetStream(&'a mut BoundaryReader),
}

impl<'a> Reader for FileEntryReader<'a> {
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

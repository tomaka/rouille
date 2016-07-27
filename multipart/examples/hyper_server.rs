extern crate hyper;
extern crate multipart;

use std::fs::File;
use std::io::{Read, Result};
use hyper::server::{Handler, Server, Request, Response};
use hyper::status::StatusCode;
use hyper::server::response::Response as HyperResponse;
use multipart::server::hyper::{Switch, MultipartHandler, HyperRequest};
use multipart::server::{Multipart, Entries, SaveResult};

struct NonMultipart;
impl Handler for NonMultipart {
    fn handle(&self, _: Request, mut res: Response) {
        *res.status_mut() = StatusCode::ImATeapot;
        res.send(b"Please send a multipart req :(\n").unwrap();
    }
}

struct EchoMultipart;
impl MultipartHandler for EchoMultipart {
    fn handle_multipart(&self, mut multipart: Multipart<HyperRequest>, mut res: HyperResponse) {
        let processing = match multipart.save_all() {
            SaveResult::Full(entries) => process_entries(entries),
            SaveResult::Partial(entries, error) => {
                println!("Errors saving multipart:\n{:?}", error);
                process_entries(entries)
            }
            SaveResult::Error(error) => {
                println!("Errors saving multipart:\n{:?}", error);
                Err(error)
            }
        };
        match processing {
            Ok(_) => res.send(b"All good in the hood :)\n").unwrap(),
            Err(_) => {
                *res.status_mut() = StatusCode::BadRequest;
                res.send(b"An error occurred :(\n").unwrap();
            }
        }
    }
}

fn process_entries(entries: Entries) -> Result<()> {
    for (name, field) in entries.fields {
        print!(r#"Field "{}": "{}""#, name, field);
    }
    for (name, savedfile) in entries.files {
        let filename = match savedfile.filename {
            Some(s) => s,
            None => "None".into()
        };
        let mut file = try!(File::open(savedfile.path));
        let mut contents = String::new();
        try!(file.read_to_string(&mut contents));

        println!(r#"Field "{}" is file "{}":"#, name, filename);
        println!("{}", contents);
    }
    Ok(())
}
fn main() {
    println!("Listening on 0.0.0.0:3333");
    Server::http("0.0.0.0:3333").unwrap().handle(
        Switch::new(
            NonMultipart,
            EchoMultipart
        )).unwrap();
}

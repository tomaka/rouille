extern crate hyper;
extern crate multipart;

use std::fs::File;
use std::io::{self, Read};
use hyper::server::{Handler, Server, Request, Response};
use hyper::status::StatusCode;
use hyper::server::response::Response as HyperResponse;
use multipart::server::hyper::{Switch, MultipartHandler, HyperRequest};
use multipart::server::{Multipart, Entries, SaveResult, SavedField};
use multipart::mock::StdoutTee;

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
        let processing = match multipart.save().temp() {
            SaveResult::Full(entries) => process_entries(&mut res, entries),
            SaveResult::Partial(entries, error) => {
                println!("Errors saving multipart:\n{:?}", error);
                process_entries(&mut res, entries.into())
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

fn process_entries(res: &mut HyperResponse, entries: Entries) -> io::Result<()> {
    let mut res = res.start();
    let stdout = io::stdout();
    entries.write_debug(StdoutTee::new(&mut res, &stdout))
}

fn main() {
    println!("Listening on 0.0.0.0:3333");
    Server::http("0.0.0.0:3333").unwrap().handle(
        Switch::new(
            NonMultipart,
            EchoMultipart
        )).unwrap();
}

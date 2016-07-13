extern crate hyper;
extern crate multipart;

use hyper::server::{Handler, Server, Request, Response};
use hyper::status::StatusCode::ImATeapot;
use hyper::server::response::Response as HyperResponse;
use multipart::server::hyper::{Switch, MultipartHandler, HyperRequest};
use multipart::server::{Multipart, Entries};

struct NonMultipart;
impl Handler for NonMultipart {
    fn handle(&self, _: Request, mut res: Response) {
        *res.status_mut() = ImATeapot;
        res.send(b"Please send a multipart req :(\n").unwrap();
    }
}

struct EchoMultipart;
impl MultipartHandler for EchoMultipart {
    fn handle_multipart(&self, multipart: Multipart<HyperRequest>, res: HyperResponse) {
        res.send(b"Thanks for the multipart req :)\n").unwrap();
    }
}

fn main() {
    Server::http("0.0.0.0:3333").unwrap().handle(
        Switch::new(
            NonMultipart,
            EchoMultipart
        )).unwrap();
    println!("Listening on 0.0.0.0:3333");
}

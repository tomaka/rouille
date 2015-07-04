#![feature(unboxed_closures, if_let, slicing_syntax)]
#![allow(dead_code)]

extern crate hyper;
extern crate multipart;

use self::hyper::server::{Listening, Server, Request, Response};
use self::hyper::client::Request as ClientReq;
use self::hyper::status::StatusCode;
use self::hyper::Url;


use self::multipart::server::Multipart;

use self::multipart::client::Multipart as ClientMulti;

use std::io::net::ip::Ipv4Addr;

use std::rand::random;

fn ok_serv(req: Request, mut res: Response) { 
    let mut multipart = Multipart::from_request(req).ok().expect("Could not create multipart!");

    multipart.foreach_entry(|&: _, _| ());
    
    *res.status_mut() = StatusCode::Ok;

    res.start().unwrap().end().unwrap();
}

thread_local!(static PORT: u16 = random())

fn server() -> Listening {
    let server = PORT.with(|port| Server::http(Ipv4Addr(127, 0, 0, 1), *port));
    server.listen(ok_serv).unwrap()
}

#[test]
fn client_api_test() {
    let mut server = server();

    let address = PORT.with(|port| format!("http://localhost:{}/", port)); 

    let request = ClientReq::post(Url::parse(&*address).unwrap()).unwrap();

    let mut multipart = ClientMulti::new();

    multipart.add_text("hello", "world");
    multipart.add_text("goodnight", "sun");
    multipart.sized = true;

    multipart.send(request).unwrap();
    
    server.close().unwrap();
}


#![feature(unboxed_closures)]

extern crate hyper;
extern crate multipart;

use self::hyper::server::{Server, Incoming};
use self::hyper::status;

use self::multipart::server::Multipart;

use std::io::net::ip::Ipv4Addr;

fn hello(mut incoming: Incoming) {
    for connection in incoming {
        let (mut req, mut res) = connection.open().ok().expect("Connection failed!");
       
        let mut multipart = Multipart::from_request(req).ok().expect("Could not create multipart!");

        multipart.foreach_entry(|&mut: name, content| println!("Name: {} Content: {}", name, content));
        
        *res.status_mut() = status::Ok;

        res.start().unwrap().end().unwrap();
    }
}

fn main() {
    let server = Server::http(Ipv4Addr(127, 0, 0, 1), 1337);
    server.listen(hello).unwrap();
    
}

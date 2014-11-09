extern crate hyper;

use self::hyper::server::{Server, Incoming};
use self::hyper::status;

use std::io::net::ip::Ipv4Addr;

fn hello(mut incoming: Incoming) {
    for (mut req, mut res) in incoming {
        *res.status_mut() = status::Ok;

        println!("{}", req.headers);
        println!("{}", req.read_to_string().unwrap());

        res.start().unwrap().end().unwrap();
    }
}

fn main() {
    let server = Server::http(Ipv4Addr(127, 0, 0, 1), 1337);
    server.listen(hello).unwrap();
}

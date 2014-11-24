#![feature(unboxed_closures, if_let, slicing_syntax)]
#![allow(dead_code)]

extern crate hyper;
extern crate multipart;

use self::hyper::server::{Server, Request, Response};
use self::hyper::status;

use self::multipart::server::Multipart;

use std::io::net::ip::Ipv4Addr;

fn hello(req: Request, mut res: Response) {  
    print_req(&req);
         
    let mut multipart = Multipart::from_request(req).ok().expect("Could not create multipart!");

    multipart.foreach_entry(|&: name, content| println!("Name: {} Content: {}", name, content));
    
    *res.status_mut() = status::Ok;

    res.start().unwrap().end().unwrap();
}

fn main() {
    let args = std::os::args();

    if args.iter().find(|s| "tcp"== s[]).is_some() {
        tcp_listen();
    } else {
        let server = Server::http(Ipv4Addr(127, 0, 0, 1), 1337);
        server.listen(hello).unwrap();
    }
}



fn tcp_listen() {
    use std::io::{Acceptor, Listener, TcpListener};
    use std::io::util::copy;

    let tcp = TcpListener::bind(("localhost", 1337u16)).unwrap();

    let ref mut stdout = std::io::stdout();

    for conn in tcp.listen().unwrap().incoming() {
       let ref mut conn = conn.unwrap();
       
       copy(conn, stdout).unwrap();
    }
}

fn print_req(req: &Request) {
    println!("Request: \nRemote addr: {}\nMethod: {}\nHeaders: {}\nURI: {}", 
        req.remote_addr, req.method, req.headers, req.uri);    
}

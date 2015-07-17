Multipart + Hyper [![Build Status](https://travis-ci.org/cybergeek94/multipart.svg?branch=master)](https://travis-ci.org/cybergeek94/multipart)
=========

Client- and server-side abstractions for HTTP file uploads (POST requests with  `Content-Type: multipart/form-data`).

Provides integration with [Hyper](https://github.com/hyperium/hyper) via the `hyper` feature. More to come!

Usage
-----

In your `Cargo.toml`:
```toml
# Currently only useful with `hyper` and `url` crates:
[dependencies]
hyper = "*"
url = "*"

[dependencies.multipart]
version = "0.2.1"
# You can also select which features to compile:
# default-features = false
# features = ["hyper", "server", "client"]
```

Client-side example using Hyper (`--features hyper,client` or default):
```rust
extern crate hyper;
extern crate multipart;
extern crate url;

use hyper::client::request::Request;
use hyper::method::Method;

use multipart::client::Multipart;

use url::Url;

fn main() {
    let url = Url::parse("127.0.0.1").unwrap();
    let request = Request::new(Method::Post, url).unwrap();
    
    let mut response = Multipart::from_request(request).unwrap()
        .write_text("hello", "world")
        .write_file("my_file", "my_text_data.txt")
        .send().unwrap();
        
    // Read response...
}
```

Server-side example using Hyper (`--features hyper,server` or default`):
```rust
use hyper::net::Fresh;
use hyper::server::{Server, Request, Response};

use multipart::server::Multipart;
use multipart::server::hyper::Switch;

fn handle_regular<'a, 'k>(req: Request<'a, 'k>, res: Response<'a, Fresh>) {
    // handle things here
}

fn handle_multipart<'a, 'k>(mut multi: Multipart<Request<'a, 'k>>, res: Response<'a, Fresh>) {
    multi.foreach_entry(|entry| println!("Multipart entry: {:?}", entry)).unwrap();
}

fn main() {
    Server::http("0.0.0.0:0").unwrap()
      .handle(Switch::new(handle_regular, handle_multipart))
      .unwrap();
}
```

####[Documentation][2]

[1]: https://github.com/hyperium/hyper
[2]: http://rust-ci.org/cybergeek94/multipart/doc/multipart/

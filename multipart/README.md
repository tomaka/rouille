# Multipart [![Build Status](https://travis-ci.org/abonander/multipart.svg?branch=master)](https://travis-ci.org/abonander/multipart) [![On Crates.io](https://img.shields.io/crates/v/multipart.svg)](https://crates.io/crates/multipart)

Client- and server-side abstractions for HTTP file uploads (POST requests with  `Content-Type: multipart/form-data`).

Supports several different HTTP crates.

Minimum supported Rust version: 1.17.0

### [Documentation](http://docs.rs/multipart/)

## Integrations

Example files demonstrating how to use `multipart` with these crates are available under [`examples/`](examples).

### [Hyper](http://hyper.rs) 
via the `hyper` feature (enabled by default). 

Client integration includes support for regular `hyper::client::Request` objects via `multipart::client::Multipart`, as well
as integration with the new `hyper::Client` API via `multipart::client::lazy::Multipart` (new in 0.5).

Server integration for `hyper::server::Request` via `multipart::server::Multipart`.

### [Iron](http://ironframework.io) 
via the `iron` feature.

Provides regular server-side integration with `iron::Request` via `multipart::server::Multipart`, 
as well as a convenient `BeforeMiddleware` implementation in `multipart::server::iron::Intercept`.

### [tiny\_http](https://crates.io/crates/tiny_http/)
via the `tiny_http` feature.

Provides server-side integration with `tiny_http::Request` via `multipart::server::Multipart`.

### [Nickel](http://nickel.rs/) 

**Note**: Moved to `multipart-nickel` crate, see [nickel/examples/nickel.rs](nickel/examples/nickel.rs)
for updated integration example.

Provides server-side integration with `&mut nickel::Request` via `multipart::server::Multipart`. 

## License


Licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.

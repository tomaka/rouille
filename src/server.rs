// Copyright (c) 2016 The Rouille developers
// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT
// license <LICENSE-MIT or http://opensource.org/licenses/MIT>,
// at your option. All files in the project carrying such
// notice may not be copied, modified, or distributed except
// according to those terms.

use std::error::Error;
use std::io::Result as IoResult;
use std::io::Read;
use std::net::SocketAddr;
use std::net::ToSocketAddrs;
use std::panic;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::ascii::AsciiExt;
use tiny_http;

use Request;
use Response;

/// A listening server.
///
/// This struct is the more manual server creation API of rouille and can be used as an alternative
/// to the `start_server` function.
///
/// The `start_server` function is just a shortcut for `Server::new` followed with `run`. See the
/// documentation of the `start_server` function for more details about the handler.
///
/// # Example
///
/// ```no_run
/// use rouille::Server;
/// use rouille::Response;
///
/// let server = Server::new("localhost:0", |request| {
///     Response::text("hello world")
/// }).unwrap();
/// println!("Listening on {:?}", server.server_addr());
/// server.run();
/// ```
pub struct Server<F> {
    server: tiny_http::Server,
    handler: Arc<AssertUnwindSafe<F>>,
}

impl<F> Server<F> where F: Send + Sync + 'static + Fn(&Request) -> Response {
    /// Builds a new `Server` object.
    ///
    /// After this function returns, the HTTP server is listening.
    ///
    /// Returns an error if there was an error while creating the listening socket, for example if
    /// the port is already in use.
    pub fn new<A>(addr: A, handler: F) -> Result<Server<F>, Box<Error + Send + Sync>>
        where A: ToSocketAddrs
    {
        let server = try!(tiny_http::Server::http(addr));

        Ok(Server {
            server: server,
            handler: Arc::new(AssertUnwindSafe(handler)),       // TODO: using AssertUnwindSafe here is wrong, but unwind safety has some usability problems in Rust in general
        })
    }

    /// Returns the address of the listening socket.
    #[inline]
    pub fn server_addr(&self) -> SocketAddr {
        self.server.server_addr()
    }

    /// Runs the server forever, or until the listening socket is somehow force-closed by the
    /// operating system.
    #[inline]
    pub fn run(self) {
        for request in self.server.incoming_requests() {
            self.process(request);
        }
    }

    /// Processes all the client requests waiting to be processed, then returns.
    ///
    /// This function executes very quickly, as each client requests that needs to be processed
    /// is processed in a separate thread.
    #[inline]
    pub fn poll(&self) {
        while let Ok(Some(request)) = self.server.try_recv() {
            self.process(request);
        }
    }

    // Internal function, called when we got a request from tiny-http that needs to be processed.
    fn process(&self, request: tiny_http::Request) {
        // We spawn a thread so that requests are processed in parallel.
        let handler = self.handler.clone();
        thread::spawn(move || {
            // Small helper struct that makes it possible to put
            // a `tiny_http::Request` inside a `Box<Read>`.
            struct RequestRead(Arc<Mutex<Option<tiny_http::Request>>>);
            impl Read for RequestRead {
                #[inline]
                fn read(&mut self, buf: &mut [u8]) -> IoResult<usize> {
                    self.0.lock().unwrap().as_mut().unwrap().as_reader().read(buf)
                }
            }

            // Building the `Request` object.
            let tiny_http_request;
            let rouille_request = {
                let url = request.url().to_owned();
                let method = request.method().as_str().to_owned();
                let headers = request.headers().iter().map(|h| (h.field.to_string(), h.value.clone().into())).collect();
                let remote_addr = request.remote_addr().clone();

                tiny_http_request = Arc::new(Mutex::new(Some(request)));

                Request {
                    url: url,
                    method: method,
                    headers: headers,
                    https: false,
                    data: Arc::new(Mutex::new(Some(Box::new(RequestRead(tiny_http_request.clone())) as Box<_>))),
                    remote_addr: remote_addr,
                }
            };

            // Calling the handler ; this most likely takes a lot of time.
            // If the handler panics, we build a dummy response.
            let mut rouille_response = {
                // We don't use the `rouille_request` anymore after the panic, so it's ok to assert
                // it's unwind safe.
                let rouille_request = AssertUnwindSafe(rouille_request);
                let res = panic::catch_unwind(move || {
                    let rouille_request = rouille_request;
                    handler(&rouille_request)
                });

                match res {
                    Ok(r) => r,
                    Err(_) => {
                        Response::html("<h1>Internal Server Error</h1>\
                                        <p>An internal error has occurred on the server.</p>")
                            .with_status_code(500)
                    }
                }
            };

            // writing the response
            let (res_data, res_len) = rouille_response.data.into_reader_and_size();
            let mut response = tiny_http::Response::empty(rouille_response.status_code)
                                            .with_data(res_data, res_len);

            let mut upgrade_header = "".into();

            for (key, value) in rouille_response.headers {
                if key.eq_ignore_ascii_case("Content-Length") {
                    continue;
                }

                if key.eq_ignore_ascii_case("Upgrade") {
                    upgrade_header = value;
                    continue;
                }

                if let Ok(header) = tiny_http::Header::from_bytes(key.as_bytes(), value.as_bytes()) {
                    response.add_header(header);
                } else {
                    // TODO: ?
                }
            }

            if let Some(ref mut upgrade) = rouille_response.upgrade {
                let trq = tiny_http_request.lock().unwrap().take().unwrap();
                let socket = trq.upgrade(&upgrade_header, response);
                upgrade.build(socket);

            } else {
                // We don't really care if we fail to send the response to the client, as there's
                // nothing we can do anyway.
                let _ = tiny_http_request.lock().unwrap().take().unwrap().respond(response);
            }
        });
    }
}

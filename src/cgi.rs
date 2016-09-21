// Copyright (c) 2016 The Rouille developers
// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT
// license <LICENSE-MIT or http://opensource.org/licenses/MIT>,
// at your option. All files in the project carrying such
// notice may not be copied, modified, or distributed except
// according to those terms.

use std::io;
use std::io::BufRead;
use std::io::Read;
use std::process::Command;
use std::process::Stdio;

use Request;
use Response;
use ResponseBody;

pub trait CgiRun {
    fn start_cgi(self, request: &Request) -> Result<Response, io::Error>;
}

impl CgiRun for Command {
    fn start_cgi(mut self, request: &Request) -> Result<Response, io::Error> {
        self.env("SERVER_SOFTWARE", "rouille")
            .env("SERVER_NAME", "localhost")            // FIXME:
            .env("GATEWAY_INTERFACE", "CGI/1.1")
            .env("SERVER_PROTOCOL", "HTTP/1.1")         // FIXME:
            .env("SERVER_PORT", "80")                   // FIXME:
            .env("REQUEST_METHOD", request.method())
            .env("PATH_INFO", &request.url())           // TODO: incorrect + what about PATH_TRANSLATED?
            .env("SCRIPT_NAME", "")                     // FIXME:
            .env("QUERY_STRING", request.raw_query_string())
            .env("REMOTE_ADDR", &request.remote_addr().to_string())
            .env("AUTH_TYPE", "")                       // FIXME:
            .env("REMOTE_USER", "")                     // FIXME:
            .env("CONTENT_TYPE", &request.header("Content-Type").unwrap_or(String::new()))
            .env("CONTENT_LENGTH", &request.header("Content-Length").unwrap_or(String::new()))
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .stdin(Stdio::piped());

        // TODO: `HTTP_` env vars with the headers

        let mut child = try!(self.spawn());
        try!(io::copy(&mut io::Cursor::new(request.data()), child.stdin.as_mut().unwrap()));

        let response = {
            let mut stdout = io::BufReader::new(child.stdout.take().unwrap());

            let mut headers = Vec::new();
            let mut status = 200;
            for header in stdout.by_ref().lines() {
                let header = try!(header);
                if header.is_empty() { break; }
    
                let mut splits = header.splitn(2, ':');
                let header = splits.next().unwrap();        // TODO: return Err instead?
                let val = splits.next().unwrap();           // TODO: return Err instead?
                let val = &val[1..];

                if header == "Status" {
                    status = val[0..3].parse().expect("Status returned by CGI program is invalid");
                } else {
                    headers.push((header.to_owned(), val.to_owned()));
                }
            }

            Response {
                status_code: status,
                headers: headers,
                data: ResponseBody::from_reader(stdout),
            }
        };

        Ok(response)
    }
}

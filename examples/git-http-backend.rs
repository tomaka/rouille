// Copyright (c) 2016 The Rouille developers
// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT
// license <LICENSE-MIT or http://opensource.org/licenses/MIT>,
// at your option. All files in the project carrying such
// notice may not be copied, modified, or distributed except
// according to those terms.

#[macro_use]
extern crate rouille;

use std::env;
use std::process::Command;
use rouille::cgi::CgiRun;

fn main() {
    rouille::start_server("localhost:8080", move |request| {
        let mut cmd = Command::new("git");
        cmd.arg("http-backend");
        cmd.env("GIT_PROJECT_ROOT", env::current_dir().unwrap().to_str().unwrap());
        cmd.env("GIT_HTTP_EXPORT_ALL", "");
        cmd.start_cgi(&request).unwrap()
    });
}

// Copyright (c) 2016 The Rouille developers
// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT
// license <LICENSE-MIT or http://opensource.org/licenses/MIT>,
// at your option. All files in the project carrying such
// notice may not be copied, modified, or distributed except
// according to those terms.

extern crate rouille;

use rouille::AssetMatch::*;
use rouille::Response;

fn main() {
    // This example shows how to serve static files with rouille.

    // Note that like all examples we only listen on `localhost`, so you can't access this server
    // from another machine than your own.
    println!("Now listening on localhost:8000");

    rouille::start_server("localhost:8000", move |request| {
        println!("\nRequest: {}", request.url());
        {
            // The `match_assets` function tries to find a file whose name corresponds to the URL
            // of the request. The second parameter (`"."`) tells where the files to look for are
            // located.
            // In order to avoid potential security threats, `match_assets` will never return any
            // file outside of this directory even if the URL is for example `/../../foo.txt`.
            let response = rouille::match_assets(request, ".");

            // If a file is found, the `match_assets` function will return a response with a 200
            // status code and the content of the file. If no file is found, it will instead return
            // an empty 404 response.
            // Here we check whether if a file is found, and if so we return the response.
            if response.is_success() {
                println!("Success, serving file");
                return response;
            }
        }

        // We haven't found a file.
        // Lets look at various options that could be offered by scanning for a viable resource.
        // Instead of a response `find_assets` returns a enum for further processing.
        let asset = rouille::find_assets(request, ".");
        println!("{asset:?}");

        match asset {
            FoundAsset(path_buf) => {
                //There is a single file at the resource, let's offer it.
                println!("Serving: {}", path_buf.display());
                return rouille::serve_asset(request, &path_buf);
            }
            FoundDirectory(path_buf) => {
                //There is a directory at the resource.
                //Maybe there is a index.html available?
                //Lets try it.
                let response = rouille::serve_asset(request, &path_buf.join("index.html"));
                if response.is_success() {
                    println!("Serving: {}", &path_buf.join("index.html").display());
                    return response;
                }
            }
            FoundMultiple(stuff) => {
                //Hey we found a bunch
                //Let's look for a html and serve it
                for entry in stuff {
                    if entry.extension().unwrap() == "html" {
                        println!("Serving: {}", entry.display());
                        return rouille::serve_asset(request, &entry);
                    }
                }
                //Alternatively files like various template formats or .md could be handed over to a renderer.
                //E.g. Look at the comrak crate for a Markdown renderer.
            }
            FoundNone => (),
        }

        // This point of the code is reached only if no static file matched the request URL.

        // In a real website you probably want to serve non-static files here (with the `router!`
        // macro for example), but here we just return a 404 response.
        println!("Failed to serve anything");
        Response::html(
            "404 error. Try <a href=\"/README.md\"`>README.md</a> or \
                        <a href=\"/src/lib.rs\">src/lib.rs</a> for example.",
        )
        .with_status_code(404)
    });
}

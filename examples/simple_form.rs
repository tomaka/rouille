extern crate mustache;
#[macro_use]
extern crate rouille;
extern crate rustc_serialize;

use std::io;

fn main() {
    let server = rouille::Server::start();

    let form = mustache::compile_path("./examples/assets/form.html.mustache").unwrap();
    let form_success = mustache::compile_path("./examples/assets/form_success.html.mustache").unwrap();

    for request in server {
        let _entry = rouille::LogEntry::start(io::stdout(), &request);

        let response = router!(request,
            GET (/) => (|| {
                let mut output = Vec::new();
                form.render_data(&mut output, &mustache::Data::Bool(false));
                Ok(rouille::Response::html(&output))
            }),

            GET (/submit) => (|| {
                let data: FormData = try!(rouille::input::get_post_input(&request));
                println!("{:?}", data);

                let mut output = Vec::new();
                form_success.render(&mut output, &data);
                Ok(rouille::Response::html(&output))
            }),

            _ => || Err(rouille::RouteError::NoRouteFound)
        );

        match response {
            Ok(r) => request.respond(r),
            Err(err) => request.respond_to_error(&err),
        };
    }
}

#[derive(Debug, RustcEncodable, RustcDecodable)]
struct FormData {
    login: String,
    password: String,
}

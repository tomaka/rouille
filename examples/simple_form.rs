extern crate mustache;
#[macro_use]
extern crate rouille;
extern crate rustc_serialize;

use std::io;

fn main() {
    let form = mustache::compile_path("./examples/assets/form.html.mustache").unwrap();
    let form_success = mustache::compile_path("./examples/assets/form_success.html.mustache").unwrap();

    rouille::start_server("localhost:8000", move |request| {
        let _entry = rouille::LogEntry::start(io::stdout(), request);

        let response = router!(request,
            (GET) (/) => (|| {
                let mut output = Vec::new();
                form.render_data(&mut output, &mustache::Data::Bool(false));
                Ok(rouille::Response::html(output))
            }),

            (POST) (/submit) => (|| {
                let data: FormData = try!(rouille::input::get_post_input(request));
                println!("{:?}", data);

                let mut output = Vec::new();
                form_success.render(&mut output, &data);
                Ok(rouille::Response::html(output))
            }),

            _ => || Err(rouille::RouteError::NoRouteFound)
        );

        response.unwrap_or_else(|err| rouille::Response::from_error(&err))
    });
}

#[derive(Debug, RustcEncodable, RustcDecodable)]
struct FormData {
    login: String,
    password: String,
}

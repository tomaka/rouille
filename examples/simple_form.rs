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

        router!(request,
            (GET) (/) => {
                let mut output = Vec::new();
                form.render_data(&mut output, &mustache::Data::Bool(false));
                rouille::Response::html(output)
            },

            (POST) (/submit) => {
                let data: FormData = try_or_400!(rouille::input::get_post_input(request));
                println!("{:?}", data);

                let mut output = Vec::new();
                form_success.render(&mut output, &data);
                rouille::Response::html(output)
            },

            _ => rouille::Response::empty_404()
        )
    });
}

#[derive(Debug, RustcEncodable, RustcDecodable)]
struct FormData {
    login: String,
    password: String,
}

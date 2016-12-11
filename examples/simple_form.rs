extern crate mustache;
#[macro_use]
extern crate rouille;
extern crate rustc_serialize;

use std::io;

fn main() {
    let form = mustache::compile_path("./examples/assets/form.html.mustache").unwrap();
    let form_success = mustache::compile_path("./examples/assets/form_success.html.mustache").unwrap();

    rouille::start_server("localhost:8000", move |request| {
        rouille::log(&request, io::stdout(), || {
            router!(request,
                (GET) (/) => {
                    let mut output = Vec::new();
                    form.render_data(&mut output, &mustache::Data::Bool(false));
                    rouille::Response::html(String::from_utf8(output).unwrap())
                },

                (POST) (/submit) => {
                    let data = try_or_400!(post_input!(request, {
                        login: String,
                        password: String,
                    }));

                    println!("{:?}", data);

                    #[derive(Debug, RustcEncodable)]
                    struct TemplateOut { login: String, }
                    let template_out = TemplateOut { login: data.login, };

                    let mut output = Vec::new();
                    form_success.render(&mut output, &template_out).unwrap();
                    rouille::Response::html(String::from_utf8(output).unwrap())
                },

                _ => rouille::Response::empty_404()
            )
        })
    });
}

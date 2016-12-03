#[macro_use]
extern crate rouille;
extern crate rustc_serialize;

use std::io;

fn main() {
    rouille::start_server("localhost:8001", move |request| {
        rouille::log(&request, io::stdout(), || {
            router!(request,
                (GET) (/) => {
                    rouille::Response::html(FORM)
                },

                (POST) (/submit) => {
                    let data = try_or_400!(post_input!(request, {
                        name: String,
                        file: Vec<rouille::input::post::BufferedFile>,
                    }));

                    println!("{:?}", data);

                    rouille::Response::html(FORM)
                },

                _ => rouille::Response::empty_404()
            )
        })
    });
}

static FORM: &'static str = r#"
<html>
    <head>
        <title>Form</title>
    </head>
    <body>
        <form action="submit" method="POST" enctype="multipart/form-data">
            <p><input type="text" name="name" placeholder="Some text" /></p>

            <p><input type="file" name="file" multiple /></p>

            <p><button>Upload</button></p>
        </form>
    </body>
</html>

"#;

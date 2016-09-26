#[macro_use]
extern crate rouille;
extern crate rustc_serialize;

use std::io;
use std::io::Read;

fn main() {
    rouille::start_server("localhost:8001", move |request| {
        let _entry = rouille::LogEntry::start(io::stdout(), request);

        router!(request,
            (GET) (/) => {
                rouille::Response::html(FORM)
            },

            (POST) (/submit) => {
                let mut multipart = rouille::input::multipart::get_multipart_input(&request)
                                                                                        .unwrap();

                while let Some(entry) = multipart.next() {
                    println!("{:?}", entry.name);

                    match entry.data {
                        rouille::input::multipart::MultipartData::Text(txt) => println!("{:?}", txt),
                        rouille::input::multipart::MultipartData::File(mut f) => {
                            let mut data = Vec::new();
                            f.read_to_end(&mut data).unwrap();
                            println!("{:?}", data)
                        },
                    }
                }

                rouille::Response::html(FORM)
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

static FORM: &'static str = r#"
<html>
    <head>
        <title>Form</title>
    </head>
    <body>
        <form action="submit" method="POST" enctype="multipart/form-data">
            <p><input type="text" name="text" placeholder="Some text" /></p>

            <p><input type="file" name="file" /></p>

            <p><button>Upload</button></p>
        </form>
    </body>
</html>

"#;

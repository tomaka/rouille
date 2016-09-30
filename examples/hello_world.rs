#[macro_use]
extern crate rouille;

use std::io;

fn main() {
    rouille::start_server("localhost:8000", move |request| {
        let _entry = rouille::LogEntry::start(io::stdout(), request);

        if let Some(request) = request.remove_prefix("/examples") {
            let response = rouille::match_assets(&request, "examples");
            if response.success() {
                return response;
            }
        }

        router!(request,
            (GET) (/) => {
                rouille::Response::redirect("/hello/world")
            },

            (GET) (/hello/world) => {
                println!("hello world");
                rouille::Response::text("hello world")
            },

            (GET) (/hello-world) => {
                println!("hello-world");
                rouille::Response::text("hello world")
            },

            (GET) (/panic) => {
                panic!("Oops!")
            },

            (GET) (/{id: u32}) => {
                println!("u32 {:?}", id);
                rouille::Response::empty_400()
            },

            (GET) (/{id: String}) => {
                println!("String {:?}", id);
                rouille::Response::text(format!("hello, {}", id))
            },

            _ => rouille::Response::empty_404()
        )
    });
}

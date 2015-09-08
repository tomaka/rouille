#[macro_use]
extern crate rouille;

use std::io;

fn main() {
    rouille::start_server("localhost:8000", move |request| {
        let _entry = rouille::LogEntry::start(io::stdout(), request);

        if let Ok(r) = rouille::match_assets(request, "examples") {
            return r;
        }

        let response = router!(request,
            (GET) (/) => (|| {
                Ok(rouille::Response::redirect("/hello/world"))
            }),

            (GET) (/hello/world) => (|| {
                println!("hello world");
                Ok(rouille::Response::text("hello world"))
            }),

            (GET) (/panic) => (|| {
                panic!("Oops!")
            }),

            (GET) (/{id}) => (|id: u32| {
                println!("u32 {:?}", id);
                Err(rouille::RouteError::WrongInput)
            }),

            (GET) (/{id}) => (|id: String| {
                println!("String {:?}", id);
                Ok(rouille::Response::text(format!("hello, {}", id)))
            }),

            _ => || Err(rouille::RouteError::NoRouteFound)
        );

        response.unwrap_or_else(|err| rouille::Response::from_error(&err))
    });
}

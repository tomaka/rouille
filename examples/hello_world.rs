#[macro_use]
extern crate rouille;

use std::io;

fn main() {
    let server = rouille::Server::start();

    for request in server {
        let _entry = rouille::LogEntry::start(io::stdout(), &request);

        if let Ok(r) = rouille::match_assets(&request, "examples") {
            request.respond(r);
            continue;
        }

        let response = router!(request,
            GET (/) => (|| {
                println!("test qsdf");
                Err(rouille::RouteError::WrongInput)
            }),

            GET (/hello/world) => (|| {
                println!("hello world");
                Err(rouille::RouteError::WrongInput)
            }),

            GET (/{id}) => (|id: u32| {
                println!("u32 {:?}", id);
                Err(rouille::RouteError::WrongInput)
            }),

            GET (/{id}) => (|id: String| {
                println!("String {:?}", id);
                Err(rouille::RouteError::WrongInput)
            }),

            _ => || Err(rouille::RouteError::NoRouteFound)
        );

        match response {
            Ok(r) => request.respond(r),
            Err(err) => request.respond_to_error(&err),
        };
    }
}

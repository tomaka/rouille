extern crate rouille;

fn main() {
    let router = rouille::route::Router {
        routes: vec![
            rouille::route::Route {
                url: "/".to_string(),
                method: rouille::route::MethodsMask { get: true, post: false, put: false, delete: false },
                handler: rouille::route::Handler::Dynamic(Box::new(handler as fn(rouille::input::Ignore) -> rouille::output::plain_text::PlainTextOutput)),
            }
        ],
    };

    rouille::start("0.0.0.0:8000", router);
}

fn handler(_: rouille::input::Ignore) -> rouille::output::plain_text::PlainTextOutput {
    rouille::output::plain_text::PlainTextOutput::new("hello world")
}

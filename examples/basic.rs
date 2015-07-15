#[macro_use]
extern crate rouille;

fn main() {
    let router = router! {
        GET "/" => handler as fn(_) -> _
    };

    rouille::start("0.0.0.0:8000", router);
}

fn handler(_: rouille::input::Ignore) -> rouille::output::plain_text::PlainTextOutput {
    rouille::output::plain_text::PlainTextOutput::new("hello world")
}

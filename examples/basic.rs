#[macro_use]
extern crate rouille;
extern crate rustc_serialize;

#[derive(RustcEncodable)]
struct Data {
    val1: u32,
}

fn main() {
    let router = router! {
        GET "/" => handler as fn(_) -> _
    };

    rouille::start("0.0.0.0:8000", router);
}

fn handler(_: rouille::input::Ignore) -> rouille::output::json::JsonOutput<Data> {
    rouille::output::json::JsonOutput::new(Data { val1: 3 })
}

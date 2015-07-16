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

    let services = rouille::service::StaticServices {
        templates: rouille::service::TemplatesCache::new("."),
        .. Default::default()
    };

    rouille::start("0.0.0.0:8000", router, services);
}

#[derive(RustcEncodable)]
struct TemplateVars;

fn handler(_: rouille::input::Ignore) -> rouille::output::TemplateOutput<TemplateVars> {
    rouille::output::TemplateOutput::new("test", TemplateVars)
}

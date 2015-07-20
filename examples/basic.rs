#![recursion_limit = "500"]

#[macro_use]
extern crate rouille;
extern crate rustc_serialize;

fn main() {
    let router = router! {
        GET /{id} [RouteParams] => handler as fn(_) -> _,
    };

    let services = rouille::service::StaticServices {
        templates: rouille::service::TemplatesCache::new("examples"),
        .. Default::default()
    };

    rouille::start("0.0.0.0:8000", router, "examples", services);
}

#[derive(RustcEncodable)]
struct TemplateVars {
    id: u32,
}

#[derive(Debug)]
struct RouteParams {
    id: u32,
}

fn handler(_: rouille::input::Ignore)
           -> rouille::output::TemplateOutput<TemplateVars>
{
    //println!("{:?}", params);
    rouille::output::TemplateOutput::new("test", TemplateVars { id: 3 })
}

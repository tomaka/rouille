use std::io::Write;

use hyper::server::response::Response as HyperResponse;
use rustc_serialize::Encodable;
use service::StaticServices;
use super::Output;

pub struct TemplateOutput<D> {
	pub template_name: String,
    pub data: D,
}

impl<D> TemplateOutput<D> {
    pub fn new<S>(template: S, data: D) -> TemplateOutput<D> where S: Into<String> {
        TemplateOutput {
            template_name: template.into(),
            data: data,
        }
    }
}

impl<D> Output for TemplateOutput<D> where D: Encodable {
    fn send(self, mut response: HyperResponse, services: &StaticServices) {
        use hyper::header::ContentType;
        use hyper::mime::{Mime, TopLevel, SubLevel, Attr, Value};

        let h = ContentType(Mime(TopLevel::Text, SubLevel::Html,
                                 vec![(Attr::Charset, Value::Utf8)]));
        response.headers_mut().set(h);

        let mut response = match response.start() {
            Ok(r) => r,
            Err(_) => return
        };

        let _ = services.templates.render(&self.template_name, response.by_ref(), &self.data);
        let _ = response.end();
    }
}

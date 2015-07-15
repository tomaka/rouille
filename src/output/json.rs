use hyper::server::response::Response as HyperResponse;
use rustc_serialize::Encodable;
use rustc_serialize::json;
use super::Output;

/// A JSON output.
pub struct JsonOutput<D> {
    /// The data to write in the response.
    pub data: D,
}

impl<D> JsonOutput<D> {
    /// Builds a `JsonOutput`.
    pub fn new(data: D) -> JsonOutput<D> where D: Encodable {
        JsonOutput {
            data: data,
        }
    }
}

impl<D> Output for JsonOutput<D> where D: Encodable {
    fn send(self, mut response: HyperResponse) {
        use hyper::header::ContentType;
        use hyper::mime::{Mime, TopLevel, SubLevel, Attr, Value};

        let h = ContentType(Mime(TopLevel::Application, SubLevel::Json,
                                 vec![(Attr::Charset, Value::Utf8)]));
        response.headers_mut().set(h);

        let encoded = json::encode(&self.data).unwrap();
        let _ = response.send(encoded.as_bytes());
    }
}

use hyper::server::response::Response as HyperResponse;
use super::Output;

/// A plain-text output.
pub struct PlainTextOutput {
    /// The text to write to the output.
    pub data: String,
}

impl PlainTextOutput {
    /// Builds a `PlainTextOutput`.
    ///
    /// ```rust
    /// use rouille::output::plain_text::PlainTextOutput;
    /// let output = PlainTextOutput::new("hello world");
    /// ```
    pub fn new<S>(data: S) -> PlainTextOutput where S: Into<String> {
        PlainTextOutput {
            data: data.into(),
        }
    }
}

impl Output for PlainTextOutput {
    fn send(self, mut response: HyperResponse) {
        use hyper::header::ContentType;
        use hyper::mime::{Mime, TopLevel, SubLevel, Attr, Value};

        let h = ContentType(Mime(TopLevel::Text, SubLevel::Plain,
                                 vec![(Attr::Charset, Value::Utf8)]));
        response.headers_mut().set(h);

        let _ = response.send(self.data.as_bytes());
    }
}

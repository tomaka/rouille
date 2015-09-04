use rustc_serialize::base64::FromBase64;
use Request;

pub use self::post::get_post_input;

pub mod post;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpAuthCredentials {
    pub login: String,
    pub password: String,
}

/// Attempts to parse a `Authorization` header with basic HTTP auth.
///
/// If such a header is present a valid, a `HttpAuthCredentials` is returned.
pub fn get_basic_http_auth(request: &Request) -> Option<HttpAuthCredentials> {
    let header = match request.header("Authorization") {
        None => return None,
        Some(h) => h,
    };

    let mut split = header.splitn(2, |c| c == ' ');
    let authtype = match split.next() { None => return None, Some(t) => t };

    if authtype != "Basic" {
        return None;
    }

    let authvalue = match split.next() { None => return None, Some(v) => v };
    let authvalue = match authvalue.from_base64() { Ok(v) => v, Err(_) => return None };

    let mut split = authvalue.splitn(2, |&c| c == b':');
    let login = match split.next() { Some(l) => l, None => return None };
    let pass = match split.next() { Some(p) => p, None => return None };

    let login = match String::from_utf8(login.to_owned()) { Ok(l) => l, Err(_) => return None };
    let pass = match String::from_utf8(pass.to_owned()) { Ok(p) => p, Err(_) => return None };

    Some(HttpAuthCredentials {
        login: login,
        password: pass,
    })
}

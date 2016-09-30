// Copyright (c) 2016 The Rouille developers
// Licensed under the Apache License, Version 2.0
// <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT
// license <LICENSE-MIT or http://opensource.org/licenses/MIT>,
// at your option. All files in the project carrying such
// notice may not be copied, modified, or distributed except
// according to those terms.

use rustc_serialize::base64::FromBase64;
use Request;

pub use self::json::get_json_input;
pub use self::post::get_post_input;
pub use self::session::{SessionsManager, Session, generate_session_id};

pub mod json;
pub mod multipart;
pub mod post;

mod session;

/// Credentials returned by `get_basic_http_auth`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpAuthCredentials {
    /// Login provided by the client.
    pub login: String,
    /// Password provided by the client.
    pub password: String,
}

/// Attempts to parse a `Authorization` header with basic HTTP auth.
///
/// If such a header is present and valid, a `HttpAuthCredentials` is returned.
///
/// # Example
///
/// ```
/// use rouille::input;
/// use rouille::Request;
/// use rouille::Response;
///
/// fn handle(request: &Request) -> Response {
///     let auth = match input::get_basic_http_auth(request) {
///         Some(a) => a,
///         None => return Response::basic_http_auth_login_required("realm")
///     };
///
///     if auth.login == "admin" && auth.password == "GT5GeKyLvKLxuc7mjF5h" {
///         handle_after_login(request)
///     } else {
///         Response::text("Bad login/password").with_status_code(403)
///     }
/// }
///
/// fn handle_after_login(request: &Request) -> Response {
///     Response::text("You are in a secret area")
/// }
/// ```
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

/// Attempts to parse the list of cookies from the request.
///
/// Returns a pair of `(key, value)`. If the header is missing or malformed, an empty
/// `Vec` is returned.
// TODO: should an error be returned if the header is malformed?
// TODO: be less tolerent to what is accepted?
pub fn get_cookies(request: &Request) -> Vec<(String, String)> {
    let header = match request.header("Cookie") {
        None => return Vec::new(),
        Some(h) => h,
    };

    header
        .split(|c| c == ';')
        .filter_map(|cookie| {
            let mut splits = cookie.splitn(2, |c| c == '=');
            let key = match splits.next() { None => return None, Some(v) => v };
            let value = match splits.next() { None => return None, Some(v) => v };

            let key = key.trim().to_owned();
            let value = value.trim().trim_matches(|c| c == '"').to_owned();

            Some((key, value))
        })
        .collect()
}

#[cfg(test)]
mod test {
    use Request;
    use super::HttpAuthCredentials;
    use super::get_basic_http_auth;
    use super::get_cookies;

    #[test]
    fn basic_http_auth_no_header() {
        let request = Request::fake_http("GET", "/", vec![], Vec::new());
        assert_eq!(get_basic_http_auth(&request), None);
    }

    #[test]
    fn basic_http_auth_wrong_header() {
        let request = Request::fake_http("GET", "/",
                                         vec![("Authorization".to_owned(),
                                               "hello world".to_owned())],
                                         Vec::new());
        assert_eq!(get_basic_http_auth(&request), None);

        let request = Request::fake_http("GET", "/",
                                         vec![("Authorization".to_owned(),
                                               "Basic \0\0".to_owned())],
                                         Vec::new());
        assert_eq!(get_basic_http_auth(&request), None);
    }

    #[test]
    fn basic_http_auth_ok() {
        let request = Request::fake_http("GET", "/",
                                         vec![("Authorization".to_owned(),
                                               "Basic QWxhZGRpbjpvcGVuIHNlc2FtZQ==".to_owned())],
                                         Vec::new());

        assert_eq!(get_basic_http_auth(&request), Some(HttpAuthCredentials {
            login: "Aladdin".to_owned(),
            password: "open sesame".to_owned(),
        }));
    }

    #[test]
    fn cookies_ok() {
        let request = Request::fake_http("GET", "/",
                                         vec![("Cookie".to_owned(),
                                               "a=b; hello=world".to_owned())],
                                         Vec::new());

        assert_eq!(get_cookies(&request), vec![
            ("a".to_owned(), "b".to_owned()),
            ("hello".to_owned(), "world".to_owned())
        ]);
    }
}

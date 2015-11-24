use std::collections::HashMap;
use std::sync::Mutex;
use rand;
use rand::Rng;

use Request;
use Response;
use input;

/// Manages all active user sessions in memory.
///
/// # Example
///
/// ```no_run
/// #[derive(Debug, Clone)]
/// struct SessionData {
///     user_id: i32
/// }
///
/// let sessions = rouille::SessionsManager::<SessionData>::new("SID", 3600);
/// 
/// rouille::start_server("localhost:80", move |request| {
///     let session = sessions.start(&request);
///     // rest of the handler
/// # let response: rouille::Response = unsafe { ::std::mem::uninitialized() };
///     session.apply(response)
/// })
/// ```
///
pub struct SessionsManager<T> where T: Clone {
    // TODO: eventually replace the key with `[u8; 64]` or something similar
    sessions: Mutex<HashMap<String, T>>,
    cookie_name: String,
    timeout_s: u64,
}

impl<T> SessionsManager<T> where T: Clone {
    /// Initializes the sessions manager.
    ///
    /// # Parameters
    ///
    /// - `cookie_name`: The name of the cookie to use. Usually `SID`.
    /// - `timeout_s`: The duration of the session, in seconds. Usually 3600.
    ///
    pub fn new<S>(cookie_name: S, timeout_s: u64) -> SessionsManager<T> where S: Into<String> {
        SessionsManager {
            sessions: Mutex::new(HashMap::new()),
            cookie_name: cookie_name.into(),
            timeout_s: timeout_s,
        }
    }

    /// Tries to load an existing session from the request, or creates one if there
    /// is no session yet.
    pub fn start(&self, request: &Request) -> Session<T> {
        let mut cookie = input::get_cookies(request).into_iter();
        let cookie = cookie.find(|&(ref k, _)| k == &self.cookie_name);
        let cookie = cookie.map(|(k, v)| v);

        if let Some(cookie) = cookie {
            Session {
                manager: self,
                key: cookie,
            }
        } else {
            Session {
                manager: self,
                key: generate_session_id(),
            }
        }
    }
}

/// Represents an entry in the sessions manager.
pub struct Session<'a, T> where T: Clone + 'a {
    manager: &'a SessionsManager<T>,
    key: String,
}

impl<'a, T> Session<'a, T> where T: Clone {
    /// Load the session infos from the manager. Returns `None` if there is no data yet.
    ///
    /// Note that calling `get` twice in a row can produce different results. That can happen
    /// if two requests are processed in parallel and access the same session.
    pub fn get(&self) -> Option<T> {
        let session = self.manager.sessions.lock().unwrap();
        session.get(&self.key).map(|d| d.clone())
    }

    /// Returns true if there is session data.
    #[inline]
    pub fn has_data(&self) -> bool {
        let session = self.manager.sessions.lock().unwrap();
        session.get(&self.key).is_some()
    }

    /// Stores the session infos in the manager.
    pub fn set(&self, value: T) {
        let mut session = self.manager.sessions.lock().unwrap();
        session.insert(self.key.clone(), value);
    }

    /// Removes the session infos from the manager.
    pub fn clear(&self) {
        let mut session = self.manager.sessions.lock().unwrap();
        session.remove(&self.key);
    }

    /// Applies the session on the `Response`. If you don't do that, the session won't be
    /// maintained on further connections.
    pub fn apply(&self, mut response: Response) -> Response {
        if !self.has_data() {
            return response;
        }

        // FIXME: correct interactions with existing headers
        let header_value = format!("{}={}; Max-Age={}", self.manager.cookie_name, self.key,
                                                        self.manager.timeout_s);
        response.headers.push(("Set-Cookie".to_owned(), header_value));
        response
    }
}

/// Generates a string suitable for a session ID.
///
/// The output string doesn't contain any punctuation or character such as quotes or brackets
/// that could need to be escaped.
pub fn generate_session_id() -> String {
    // 5e+114 possibilities is reasonable
    rand::OsRng::new().unwrap()     // TODO: <- how to handle that?
                      .gen_ascii_chars()
                      .filter(|&c| (c >= 'a' && c <= 'z') || (c >= 'A' && c <= 'Z') ||
                                   (c >= '0' && c <= '9'))
                      .take(64).collect::<String>()
}

# Admin login

Let's tackle the admin panel.

```rust
fn handle_admin(request: &Request, database: &Transaction,
                sessions: &SessionsManager) -> Result<Response, RouteError>
{
    ...
}
```

The difference with the public part of the website is that only authorized users must have access
to the admin panel.

We split the admin panel in three functions:

 - `handle_admin_loggedin` that handles routes that are available if you are logged int.
 - `handle_admin_always_avail` that handles routes that are always available (like the login page).
 - `handle_admin` that handles authentication and dispatches to one of the other two functions.

```rust
struct SessionData {
    user_id: i32
}

fn handle_admin(request: &Request, database: &Transaction,
                sessions: &SessionsManager<SessionData>) -> Result<Response, RouteError>
{
    if !request.url().starts_with("/admin") {
        return Err(RouteError::NoRouteFound);
    }

    let session = sessions.start(request);

    match handle_admin_always_avail(request, database, &session) {
        Err(RouteError::NoRouteFound) => (),
        resp => return resp
    };

    let auth_user_id = match session.get() {
        Some(data) => data.user_id,
        None => return Ok(Response::redirect("/admin/login")),
    };

    handle_admin_loggedin(request, database, auth_user_id)
            .map(|response| session.apply(response))
}
```

```rust
fn handle_admin_always_avail(request: &Request, database: &Transaction,
                             session: &Session<SessionData>)
                             -> Result<Response, RouteError>
{
    router!(request,
        (GET) (/admin/login) => {

        },

        (POST) (/admin/login) => {
            #[derive(RustcEncodable)]
            struct FormData { login: String, password: String }

            let data: FormData = match rouille::input::get_post_input() {
                Ok(data) => data,
                Err(_) => Ok(Response::redirect("/admin/login"))
            };

            let user_id = match authenticate(database, data.login, data.password) {
                Ok(user_id) => user_id,
                Err(_) => Ok(Response::redirect("/admin/login_failed"))
            };

            session.set(SessionData { user_id: user_id });
            Ok(Response::redirect("/admin/login_success"));
        },

        _ => Err(RouteError::NoRouteFound)
    )
}
```

```rust
fn handle_admin_loggedin(request: &Request, database: Transaction, auth_user_id: i32)
                         -> Result<Response, RouteError>
{
    ...
}
```

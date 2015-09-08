# Routes

In the previous section, we write a pretty trivial example server:

```rust
extern crate rouille;

use rouille::Response;

fn main() {
    rouille::start_server("locahost:8000", move |request| {
        Response::text("hello world")
    })
}
```

Whenever a client queries a webpage the closure is called and `hello world` is returned.
This means that it doesn't matter whether the client visits `localhost:8000/foo`
or `localhost:8000/bar` because all that is ever returned in `hello world`.

In a real website, we want to handle requests differently depending on the request's URL,
method or headers.

## The `router!` macro

To do so, the *rouille* library provides a macro named `router!` that is every similar to
a `match` expression in Rust.

Let's see it in action:

```rust
#[macro_use]
extern crate rouille;

use rouille::Response;

fn main() {
    rouille::start_server("locahost:8000", move |request| {
        // dispatching depending on the URL and method
        router!(request,
            (GET) (/) => (|| {
                Response::text("hello from the root")
            }),

            (GET) (/foo) => (|| {
                Response::text("hello from /foo")
            }),

            _ => || Response::text("hello world")
        )
    })
}
```

Let's see what happens. Whenever a client does a request, our closure is called and the request
is passed to the `router!` macro.

The macro takes each element one by one from top to bottom. It starts by checking whether the
request's URL is `/` and its method `GET`. If it is the case, it executes the content of the
closure after the `=>`. If it's not the case, it then checks for `/foo` and `GET`. If none of
the branches match the request, then the default closure (after `_ =>`) is executed.

Now if you visit `localhost:8000` you should see `hello from the root`. If you visit
`localhost:8000/foo` you should see `hello from /foo`. And if you visit any other address you
should see `hello world`.

# Getting started

Let's get started with our first website!

Start by running `cargo new my-website`


Add a dependency to `rouille`.


```rust
extern crate rouille;

use rouille::Response;

fn main() {
    rouille::start_server("localhost:8000", move |request| {
        Response::text("hello world")
    })
}
```

The `start_server` function in *rouille* starts listening to the specifiec address and port.
The second parameter must a function or a closure that takes a reference to a `Request` and
returns a `Result`.

Once you wrote that, run `cargo run`. This should download and compile the *rouille* library
and its dependencies, after a few seconds or minutes the server should start. If you go
to [localhost:8000](http://localhost:8000/) when your server is started you should
see `hello world`!

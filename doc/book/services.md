# Services

Usually a .

## Accessing values from the outside

Accessing 

```rust
extern crate rouille;

use rouille::Response;

fn main() {
    let some_text = "hello world";

    rouille::start_server("locahost:8000", move |request| {
        Response::text(some_text)
    })
}
```

## Thread safety

However one important thing to note is that **you must handle synchronization**. The
requests-handling function that you pass to `start_server` can be called multiple times
simulatenously, therefore you can't naively modify values from the outside.

For example, let's try to implement a requests counter:

```rust
let mut counter = 0;

rouille::start_server("locahost:8000", move |request| {
    counter += 1;       // compilation error!
    Response::text(format!("Request n#{}", counter))
})
```

If this code compiled, there is a possibility that `counter` is modified twice simultaneously,
which could lead to a bad value being written in `counter`!

Instead the Rust language forces you to use a `Mutex`:

```rust
use std::sync::Mutex;

let counter = Mutex::new(0);

rouille::start_server("locahost:8000", move |request| {
    let mut counter = counter.lock().unwrap();
    // we now have an exclusive access to `counter`

    *counter += 1;
    Response::text(format!("Request n#{}", counter))
})
```

Note that in this example we could also have used a `AtomicUsize`.

# Services

To work correctly, a website usually needs access to some sort of "global service".
This includes for example:

 - A templates engine.
 - A pages cache.
 - A database.

One important part of *rouille* is that it **doesn't integrate any service** like a database or a
templating engine. The code of *rouille* is entirely dedicated to HTTP only. However its design
makes it possible to easily use any third-party library without using plugins or glue libraries.

## Accessing values from the outside

Accessing local variables created outside of the request-handling closure is easy as pie:

```rust
let some_text = "hello world";

rouille::start_server("localhost:8000", move |request| {
    Response::text(some_text)
})
```

Objects that are "global" to the server should be created on the stack before calling
`start_server`, and objects that are "per-request" should be created inside the
function.

If you get lifetime errors, make sure that you didn't forget the `move` keyword before
the closure.

## Thread safety

However one important thing to note is that **you must handle synchronization**. The
requests-handling function that you pass to `start_server` can be called multiple times
simulatenously, therefore you can't naively modify values from the outside.

For example, let's try to implement a requests counter:

```rust
let mut counter = 0;

rouille::start_server("localhost:8000", move |request| {
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

rouille::start_server("localhost:8000", move |request| {
    let mut counter = counter.lock().unwrap();
    // we now have an exclusive access to `counter`

    *counter += 1;
    Response::text(format!("Request n#{}", counter))
})
```

Note that in this example we could also have used a `AtomicUsize`.

## Example: a database connection

Let's take a concrete example by looking at how we could connect to a database. This example
assumes that you are familiar with a library that allows you to connect to a database, and
is only here to show you how to use this library in conjuction with *rouille*.

```rust
use std::sync::Mutex;

// this variable contains a cache of all the database connections
let connections = Mutex::new(Vec::new());

rouille::start_server("localhost:8000", move |request| {
    // obtaining a connection from the connections list, or creating a new one if necessary
    let connection = {
        let mut connections = connections.lock().unwrap();

        if connections.len() >= 1 {
            connections.remove(0)
        } else {
            let new_connection = Connection::new("database_url").unwrap();
            new_connection
        }
    };

    // handle the request
    let response = handle_request(request, &connection);

    // store the database connection in the cache
    connections.lock().unwrap().push(connection);

    // returning the response
    response
})
```

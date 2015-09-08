# Rouille, Rust web server middleware

## [Documentation](http://tomaka.github.io/rouille/rouille/index.html)

## [Link to WIP book](http://tomaka.github.io/rouille/book/)


## FAQ

### But I'm used to express-like frameworks!

Instead of doing this: (pseudo-code)

```js
server.add_middleware(function() {
    // middleware 1
});

server.add_middleware(function() {
    // middleware 2
});

server.add_middleware(function() {
    // middleware 3
});
```

In rouille you just handle each request entirely manually:

```rust
// initialize everything here

rouille::start_server(..., move |request| {
    // middleware 1

    // middleware 2

    // middleware 3
});
```

### What about performances?

The state of async I/O, green threads, coroutines, etc. in Rust is still blurry.

The rouille library just ignores this optimization and focuses on providing an easy-to-use
API instead.

Once async I/O has been figured out, rouille will be updated to take this into account. For the
moment it favors usability over performances.

### Are there plugins for features such as database connection, templating, etc.

It should be trivial to integrate a database or templates to your web server written with
rouille. Moreover plugins need maintenance tend to create a dependency hell. It is generally
just better not to use plugins.

### How do you know there is no defect in the API?

I'm using this library to rewrite an existing medium-sized website.

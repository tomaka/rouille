# Rouille, Rust web server middleware

## [Documentation](https://docs.rs/rouille)

[![](https://docs.rs/rouille/badge.svg)](https://docs.rs/rouille)

## [Link to WIP book](http://tomaka.github.io/rouille/book/)

## License

Licensed under either of
 * Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)
at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you shall be dual licensed as above, without any
additional terms or conditions.

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
synchronous API instead, where each request is handled in its own dedicated thread.

Even if rouille itself was asynchronous, you would need asynchronous database clients and
asynchronous file loading in order to take advantage of it. There are currently no such libraries
in the Rust ecosystem.

Once async I/O has been figured out, rouille will be (hopefully transparently) updated to take it
into account.

### Are there plugins for features such as database connection, templating, etc.

It should be trivial to integrate a database or templates to your web server written with
rouille. Moreover plugins need maintenance tend to create a dependency hell. It is generally
just better not to use plugins.

### How do you know there is no defect in the API?

I'm using this library to rewrite an existing medium-sized website.

# Rouille, Rust web server middleware




# FAQ

## But I'm used to express-like frameworks!

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
for request in requests {
    // middleware 1

    // middleware 2

    // middleware 3
}
```

## What about performances? Single threading is bad.

The state of async I/O, green threads, coroutines, etc. in Rust is still blurry.

The rouille library just ignores this optimization and provides an easy-to-use single-thready
API. However it is possible to spawn threads and use an `Arc` to share the `Server` struct
between threads.

Once async I/O has been figured out, rouille will be updated to take this into account. For the
moment it favors usability over performances.

## Why isn't it using hyper?

Since I'm a Windows developer, I'll switch to hyper only once it's easy to use on Windows.
It's simply too annoying to deal with OpenSSL at the moment.

The web server used as a backend is an implementation detail, so changing it should be
totally transparent for the user.

## Are there plugins for features such as database connection, templating, etc.

It should be trivial to integrate a database or templates to your web server written with
rouille. Moreover plugins need maintenance tend to create a dependency hell. It is generally
just better not to use plugins.

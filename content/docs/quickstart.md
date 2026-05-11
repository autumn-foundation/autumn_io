---
title: Quickstart
description: Build and run your first Autumn app.
order: 10
---

# Quickstart

Autumn is a Rust web framework for building server-rendered apps and HTTP services with typed routes, Maud templates, static assets, and production defaults in one place.

## Install

Create a new Rust binary crate and add Autumn:

```toml
[dependencies]
autumn-web = "0.4"
```

If you are working against the release branch before 0.4.0 is published, pin the dependency to the local path or Git revision used by the release candidate.

## Create an app

Start with one route and the Autumn app runner:

```rust
use autumn_web::prelude::*;

#[get("/")]
async fn index() -> Markup {
    html! {
        h1 { "Hello, Autumn." }
        p { "Your first route is alive." }
    }
}

#[autumn_web::main]
async fn main() {
    autumn_web::app()
        .routes(routes![index])
        .run()
        .await;
}
```

Run it with Cargo:

```bash
cargo run
```

Autumn binds to `127.0.0.1:3000` by default. Open `http://127.0.0.1:3000` and you should see the rendered page.

## Add a second route

Route handlers are regular async functions annotated with an HTTP method macro:

```rust
#[get("/hello/{name}")]
async fn hello_name(Path(name): Path<String>) -> Markup {
    html! {
        h1 { "Hello, " (name) "." }
    }
}
```

Register the route beside the first one:

```rust
autumn_web::app()
    .routes(routes![index, hello_name])
    .run()
    .await;
```

## Check health

Autumn exposes a health endpoint using the configured health path:

```bash
curl http://127.0.0.1:3000/health
```

Use this endpoint for local smoke checks and deployment probes.

## Next steps

Read routing next if you want to understand handler signatures and path parameters. Read configuration next if you want to change ports, logging, health paths, or production profiles.

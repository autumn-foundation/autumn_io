---
title: Routing
description: Define route handlers, path parameters, and HTML responses.
order: 20
---

# Routing

Autumn routes are async Rust functions with route macros. The macro records the HTTP method, path, handler, and metadata so the app builder can register the route cleanly.

## Route macros

Use the method macro that matches the endpoint:

```rust
use autumn_web::prelude::*;

#[get("/posts")]
async fn list_posts() -> Markup {
    html! { h1 { "Posts" } }
}

#[post("/posts")]
async fn create_post() -> &'static str {
    "created"
}
```

Collect handlers with `routes!`:

```rust
autumn_web::app()
    .routes(routes![list_posts, create_post])
    .run()
    .await;
```

## Path parameters

Use `Path<T>` to extract typed path parameters:

```rust
#[get("/posts/{id}")]
async fn show_post(Path(id): Path<i64>) -> Markup {
    html! {
        h1 { "Post " (id) }
    }
}
```

Path parameters are parsed before the handler runs. Invalid values receive an HTTP error response instead of reaching your handler with broken data.

## Return HTML

Autumn re-exports Maud, so handlers can return `Markup` directly:

```rust
#[get("/")]
async fn index() -> Markup {
    html! {
        main {
            h1 { "Autumn app" }
            p { "Server-rendered HTML, typed at compile time." }
        }
    }
}
```

For plain responses, return strings or framework response types. For explicit status codes, return an Axum-compatible response through Autumn's re-exports.

## Test routes

Autumn includes `TestApp` so route tests do not need a bound TCP port:

```rust
use autumn_web::test::TestApp;

#[tokio::test]
async fn index_renders() {
    let app = TestApp::new()
        .routes(routes![index])
        .build();

    app.get("/")
        .send()
        .await
        .assert_status(200)
        .assert_body_contains("Autumn app");
}
```

Route tests are the fastest way to prove the public surface still behaves after refactors.

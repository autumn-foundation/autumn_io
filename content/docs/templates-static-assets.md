---
title: Templates and Static Assets
description: Render typed HTML and serve CSS, JavaScript, images, and generated assets.
order: 40
---

# Templates and Static Assets

Autumn's default server-rendered path uses Maud for typed HTML and the `static/` directory for browser assets.

## Maud templates

The prelude includes `Markup` and the `html!` macro:

```rust
use autumn_web::prelude::*;

fn page_shell(title: &str, body: Markup) -> Markup {
    html! {
        (PreEscaped("<!doctype html>".to_owned()))
        html lang="en" {
            head {
                meta charset="utf-8";
                title { (title) }
            }
            body {
                (body)
            }
        }
    }
}
```

Maud escapes dynamic values by default. Use `PreEscaped` only for HTML you generated or trust.

## Static files

Files under `static/` are served from `/static`:

```text
static/
  css/site.css
  js/copy-code.js
  img/autumn-leaf.svg
```

Reference those files from templates:

```rust
html! {
    link rel="stylesheet" href="/static/css/site.css";
    script src="/static/js/copy-code.js" defer {}
}
```

## Tailwind build path

Starter apps include a Tailwind input file and build script. If the Tailwind CLI is available, the build can generate a compiled stylesheet during Cargo builds.

```css
@import "./site.css";
```

Keep durable design tokens and site-specific CSS in source-controlled files. Treat generated CSS as a build artifact unless your deployment needs it committed.

## Asset checks

Before shipping a page that references assets, verify the files resolve through the app:

```bash
curl -I http://127.0.0.1:3000/static/css/site.css
curl -I http://127.0.0.1:3000/static/img/autumn-leaf.svg
```

A beautiful stylesheet that 404s is just a ghost story with cache headers.

# Autumn Website Design

Date: 2026-05-11
Status: Approved design
Target release: Autumn 0.4.0

## Goal

Build the Autumn website with Autumn itself. The 0.4.0 launch site should be docs-first, aimed at Rust web developers who are new to Autumn and need a fast path from curiosity to a running app.

The first screen should communicate one idea clearly: Autumn helps developers build fast, simple Rust web apps with production bones. The site should feel like useful documentation first and release marketing second.

## Audience

Primary audience:

- Rust web developers evaluating Autumn for a new app.
- Developers who want to understand the framework shape in the first ten minutes.
- Users who value practical examples, typed APIs, and production-ready defaults.

Secondary audience:

- Existing Autumn users looking for 0.4.0 upgrade notes.
- Contributors who need a stable public story for the framework.

## MVP Scope

Launch with the core docs path:

- `/` - Home and docs landing page.
- `/docs/quickstart` - install, create an app, run it, add the first route.
- `/docs/routing` - route macros, path parameters, handler basics.
- `/docs/configuration` - `autumn.toml`, profiles, server/log/health settings.
- `/docs/templates-static-assets` - templates, static files, Tailwind/CSS assets.
- `/docs/deployment` - production build, Docker, health checks, environment setup.
- `/docs/upgrade-0-4` - upgrade notes, breaking changes, and migration guidance.

Search should be search-ready, not search-enabled, for the launch MVP. Pages need stable metadata, clear headings, predictable slugs, and generated heading IDs so a later search index can be added without restructuring the content.

## Content Model

Docs content should live as Markdown files under `content/docs/*.md`.

Each page should include frontmatter:

```yaml
---
title: Quickstart
description: Build and run your first Autumn app.
order: 10
---
```

The Autumn app should load these files into a docs registry, ordered by `order`. Markdown rendering should produce article HTML, a generated table of contents, previous/next page links, and metadata for the shared layout.

This keeps docs easy to review and edit while still dogfooding Autumn routes and static asset handling. Avoid embedding long-form prose directly in Rust strings.

## Information Architecture

Use a reference-docs layout:

- Top header with the custom Autumn leaf mark, wordmark, version label, GitHub link, and docs entry point.
- Left sidebar navigation for the launch docs pages.
- Center article column for rendered Markdown.
- Right in-page table of contents generated from headings.
- Previous/next links at the bottom of docs pages.
- Shared 404 page for unknown docs slugs.

`/docs` should redirect to `/docs/quickstart` or render a compact docs index that points users into the quickstart. The preferred MVP behavior is redirecting to Quickstart.

## Homepage

The homepage should be a useful docs landing page, not a generic marketing splash.

First viewport:

- Headline focused on fast, simple Rust web apps.
- Two-line explanation of Autumn's value.
- Primary action: Get started.
- Secondary action: Read the docs.
- A real Rust route example using Autumn.

Below the hero, show compact feature links:

- Routing
- Configuration
- Templates and static assets
- Deployment
- Health checks and production readiness
- 0.4.0 upgrade notes

Every feature item should link to real docs content. No decorative dead-end cards.

## Visual Direction

Use a warm seasonal brand while keeping the site credible as framework documentation.

Brand cues:

- A custom leaf mark inspired by the Autumn leaf emoji.
- Use the actual leaf emoji casually in copy where appropriate, but not as the core logo.
- The SVG mark should work in the header, favicon, and social image contexts.

Palette:

- Warm off-white background.
- Ink-dark primary text.
- Rust or copper primary accent.
- Muted moss or evergreen secondary accent.
- Neutral grays for UI chrome, borders, and code surfaces.

Avoid a one-note amber palette. The site should not look like seasonal retail. The brand should be warm, but the reading experience should stay sharp.

Typography:

- Clean sans-serif stack for UI and body text.
- Monospace stack for code.
- High contrast code blocks with horizontal overflow handling.

## Code Blocks

Code examples are central to the site.

Code blocks should support:

- Strong readable contrast.
- Optional language labels.
- Copy buttons wired by a small static JavaScript file.
- Horizontal scrolling instead of layout overflow.
- Stable markup that can be tested.

The Quickstart page should include a complete minimal route example and a command sequence to run the app.

## Runtime Architecture

Keep the runtime small and explicit.

Suggested modules:

- `docs` - Markdown loading, frontmatter parsing, docs registry, heading extraction.
- `layout` - shared HTML shell, docs chrome, homepage sections.
- `assets` - static asset route helpers if needed.

Routes:

- `/`
- `/docs`
- `/docs/{slug}`
- existing health routes from Autumn config

Unknown docs slugs should render a proper 404 response inside the site layout. They should not panic or return an unstyled plain string.

## Testing And Verification

Behavior needs tests, even for docs infrastructure.

Tests should cover:

- Frontmatter parsing.
- Docs registry ordering.
- Heading ID generation.
- Table of contents extraction.
- Previous/next link calculation.
- Home route smoke rendering.
- Docs route smoke rendering.
- Unknown docs slug 404 behavior.

At least one happy-path test should render the Quickstart page and assert:

- The Quickstart title appears.
- The left nav includes expected docs pages.
- The table of contents includes a known heading.
- A Rust code block is present.

Before calling implementation complete:

- Run `cargo fmt`.
- Run `cargo test`.
- Scan the affected area for `TODO`, `FIXME`, and stubs.
- Run the app locally and hit the homepage, Quickstart, and a missing docs page.

## Open Decisions

- Exact Autumn 0.4.0 API examples should be verified against the release branch before writing final docs.
- Final deployment target is not chosen yet.
- Search implementation is deferred until after the MVP content settles.
- Social card generation can be added later unless launch polish requires it.

---
name: autumn-docs
description: Look up the Autumn Rust web framework documentation for the release that is actually deployed, via the docs MCP server, the JSON API, or Markdown content negotiation.
---

# Autumn docs

Autumn is a Rust web framework for fast server-rendered apps. Its guides are
published at https://autumn-web.app/docs. Read them from the site instead of
recalling training data: the framework changes between releases.

## Preferred: the docs MCP server

Endpoint: `https://autumn-web.app/mcp` (public, read-only, no authentication).

```bash
claude mcp add --transport http autumn-docs https://autumn-web.app/mcp
```

Tools:

- `list_autumn_docs` — list guides (slug, title, group). Optional `group`.
- `search_autumn_docs` — ranked search. Use a few distinctive words.
- `get_autumn_doc` — a guide's Markdown by slug. Optional `section` for guides
  over the inline size limit.

## Without MCP

- `GET https://autumn-web.app/api/docs` — JSON list of guides.
- `GET https://autumn-web.app/api/search?q=<terms>` — JSON search.
- `GET https://autumn-web.app/api/docs/{slug}` — JSON with the guide's Markdown.
- `GET https://autumn-web.app/docs/{slug}` with `Accept: text/markdown` — the
  guide as Markdown.
- `https://autumn-web.app/sitemap.xml` lists every guide.

## Workflow

1. Search for the feature with `search_autumn_docs` or `/api/search`.
2. Fetch the best-matching guide by slug.
3. Answer from the guide, and cite its URL (`https://autumn-web.app/docs/{slug}`).

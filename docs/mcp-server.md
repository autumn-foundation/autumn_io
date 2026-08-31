# The docs MCP server

`https://autumn-web.app/mcp` serves this site's guides over the
[Model Context Protocol](https://modelcontextprotocol.io), so a coding agent can
search and read the Autumn documentation for the release that is actually
deployed instead of recalling whatever it saw in training.

It is public, unauthenticated, and read-only.

## Connecting an agent

Claude Code:

```bash
claude mcp add --transport http autumn-docs https://autumn-web.app/mcp
```

Any other MCP client, in its config file:

```json
{
  "mcpServers": {
    "autumn-docs": {
      "type": "http",
      "url": "https://autumn-web.app/mcp"
    }
  }
}
```

Or by hand, to check it is up:

```bash
curl -s https://autumn-web.app/mcp \
  -H 'content-type: application/json' \
  -d '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'
```

A browser-based MCP client also needs its origin in the site's
`cors.allowed_origins`: Autumn validates `Origin` on `/mcp` as DNS-rebinding
protection. Agents that are not browsers send no `Origin` and need no
configuration.

## The tools

| Tool | Does |
|---|---|
| `list_autumn_docs` | Lists the guides — slug, title, abridged description, sidebar group, Markdown size. Takes an optional `group` to list one sidebar section at a time; every response carries the group names. |
| `search_autumn_docs` | Ranked search over the guides. Every term must match, so a few distinctive words beat a sentence. Returns slugs and snippets. |
| `get_autumn_doc` | Returns a guide's Markdown by slug, with its section headings. Takes an optional `section`; see the size gate below. |

All three are `GET`s and carry `readOnlyHint`, so an agent can call them without
asking permission.

### The size gate

`get_autumn_doc` returns Markdown only while it is under
`api::MAX_INLINE_DOC_BYTES` (60 KB). Three guides are larger — `deployment` at
~150 KB is roughly forty thousand tokens, which would swamp the context of
whatever asked for it. Those come back with `markdown: null`, a `notice`, and
the `sections` list, so the caller recovers in one more call:

```jsonc
// get_autumn_doc { "slug": "deployment" }
{ "markdown": null,
  "notice": "This guide is 149 KB of Markdown, too large to return …",
  "sections": [ { "id": "prerequisites", "title": "Prerequisites", "level": 2 }, … ] }

// get_autumn_doc { "slug": "deployment", "query": { "section": "prerequisites" } }
{ "section": "prerequisites", "markdown": "## Prerequisites\n\n…" }
```

**The gate applies to a requested section too.** Two sections are over the cap
on their own (`deployment#push-button-deploy-to-your-own-server-autumn-deploy`
at 76 KB, `generators#autumn-generate-scaffold` at 73 KB), so exempting the
section path would reopen the hole on the very call the notice tells an agent to
make. When a section is withheld, `sections` lists the headings nested *inside*
it rather than the whole guide's, so the listed ids are the requests that
actually narrow things down. Each step is strictly smaller than the last, and
the recursion bottoms out on a heading with nothing nested inside it: that case
returns a truncated prefix cut at a line break, with a notice and the URL, so a
caller is never left with no body and no next step. No guide reaches that floor
today — it exists because guide content is synced from upstream and can grow.

Section ids are the anchors the rendered page already uses, so
`https://autumn-web.app/docs/deployment#prerequisites` is a working deep link to
the same text — an agent's citation lands where it says it does.

## How it is wired

There is no second app and no hand-written protocol code. `src/api.rs` holds
three ordinary Autumn handlers returning `Json<T>`, each tagged
`#[api_doc(mcp)]`, and `src/main.rs` calls `.mount_mcp(MCP_MOUNT_PATH)`. Autumn
derives the tool catalog — names, descriptions, `inputSchema`, safety
annotations — from the same `ApiDoc` metadata that drives its OpenAPI document,
and dispatches each `tools/call` back through the real router. The mechanism is
documented in the framework's own guide, at `/docs/mcp`.

Consequences worth knowing:

- **The tool schemas cannot drift from the handlers.** Changing a handler's
  argument or return type changes the advertised schema with no second edit.
- **The HTML site is not exposed.** Autumn only derives tools from routes with a
  JSON response schema, so the Maud pages are structurally ineligible.
- **The endpoints work without MCP.** `GET /api/docs`, `/api/search?q=…`, and
  `/api/docs/{slug}` are ordinary JSON endpoints usable from curl or any HTTP
  client.

Search lives at `/api/search`, not `/api/docs/search`: an exact route under
`/api/docs/` would shadow the guide with that slug, and upstream ships one
called `search`. The HTML site sidesteps the same trap by putting its search at
`/search`.

`robots.txt` disallows `/api/` and `/mcp`. They mirror content that already has
canonical HTML pages, and the clients they exist for do not read `robots.txt`.

## Changing it

`tests/mcp_docs_api.rs` covers both layers — the JSON handlers over HTTP, and
the same handlers through `/mcp` — because a tool can vanish from the catalog
(an untagged route, a return type that stopped being `Json<T>`) while its HTTP
endpoint keeps working. If you add an endpoint that should not be a tool, leave
it untagged; opt-in is the default and nothing is exposed implicitly.

# ADR-0002: Serve Markdown to agents, from the origin, on `Accept`

- **Status:** accepted
- **Date:** 2026-09-08
- **Related:** ADR-0001 (edge caching), `src/negotiate.rs`, `src/site/markdown.rs`

## Context

Half the traffic these guides are written for is not a browser. A coding agent
that fetches `https://autumn-web.app/docs/getting-started` today receives a
document head, a skip link, a site header, a sidebar of 140 links, an article
whose every code token is wrapped in a `<span style="color:…">` by syntect, a
pagination footer and a site footer — and has to reconstruct, badly, the
Markdown this site already holds in memory and rendered that page *from*.

The site already has an agent-facing surface: the JSON docs API and the MCP
server it projects (`/mcp`, ADR-less but documented in `docs/mcp-server.md`).
Both are excellent and both require the caller to speak a protocol. The agent in
a shell tool with `curl`, the crawler building an index, the framework that
fetches a URL its user pasted — none of them will call MCP, and all of them can
set a header.

`Accept: text/markdown` is the convention for exactly this. Cloudflare
implements it at the edge as
[Markdown for Agents](https://developers.cloudflare.com/fundamentals/reference/markdown-for-agents/),
converting HTML to Markdown on the way out; the
[markdown-negotiation skill](https://isitagentready.com/.well-known/agent-skills/markdown-negotiation/SKILL.md)
states the contract a site should meet:

- a request naming `text/markdown` gets a Markdown representation;
- the response says `Content-Type: text/markdown`;
- HTML stays the default for everything else;
- an `x-markdown-tokens` header carries the token count if one is available.

## Decision

**Negotiate at the origin, and answer with the source rather than a
conversion.**

Every page in the read path — `/`, `/docs/{slug}` (including its `404`),
`/search` — resolves `Accept` and serves either the HTML page or a Markdown
representation of the same resource, at the same URL. `src/negotiate.rs` owns
the resolution and the response headers; `src/site/markdown.rs` owns what the
Markdown says, function for function beside the `render_*` it mirrors.

Three things follow from doing this here rather than at the edge:

**The body is the author's Markdown, not a re-derivation of it.** `DocPage`
already keeps the guide's source next to the rendered HTML (it is what the MCP
`get_autumn_doc` tool returns). A guide's Markdown representation is that
string, framed by a `# Title`, the frontmatter description when the body does
not already open with it, and a footer of absolute links — previous, next, and
where the page lives on the web. No conversion, so nothing to be lossy about:
fenced code blocks arrive as the author fenced them.

**The framing suits the reader.** An agent has no sidebar to browse, so `/` and
the `404` carry the site's whole navigation as a linked, described list of every
guide — the structure the sidebar encodes, in the form a caller can act on.

**The resolution policy is the framework's, with one departure.**
`autumn_web::negotiate::Negotiate` resolves HTML against JSON with an RFC 7231
§5.3 q-value walk, and hard-codes that pair behind a `pub(crate)` parser. This
module reimplements the *parse* for the HTML/Markdown pair and copies the
*policy* — effective q from the most specific matching range, `q=0` as an
exclusion rather than a demotion, ties to the earlier entry, `406` when
everything is forbidden — so the site does not answer `Accept` two different
ways depending on the route.

The departure: **the origin's answer depends only on whether the header names
`text/markdown`.** A wildcard can forbid Markdown but never elect it, and a
refusal that never mentions Markdown is served HTML rather than `406`:

| `Accept` | Strict effective-q reading | Served |
| --- | --- | --- |
| `text/html;q=0.1, */*;q=1` | Markdown, lifted by the wildcard | HTML |
| `text/html;q=0` | Markdown, the only one not refused | HTML |
| `*/*;q=0` | `406`, everything refused | HTML |
| `text/html;q=0, text/markdown;q=0` | `406` | `406` |

That is a concession to the edge, and it is the reason the bypass rule below can
be trusted. Cloudflare cannot evaluate "the effective q of `text/markdown`
exceeds that of `text/html`"; a cache rule matches the header as a string. Any
origin decision the rule cannot recognise is a decision a warm colo overrides
with cached HTML — so the origin only makes decisions the rule can see. The
rows above are that principle applied twice: Markdown is elected only by name,
and a `406` is promised only where naming `text/markdown` has already tripped
the bypass, so the request actually arrives. RFC 7231 §6.5.6 permits the other
rows in as many words — send `406` **or** "disregard the Accept header field by
treating the response as if it is not subject to content negotiation".

What it costs is honouring `q=0` on headers nobody sends. `text/markdown`,
`text/markdown, */*;q=0.1`, and every browser and `curl` default are unaffected.

The practical effect is that nothing changes for anyone who was not asking: a
browser sends `text/html,…,*/*;q=0.8` and a bare `curl` sends `*/*`, and both
still get the page. `text/*` names both representations through one entry, so it
is not a preference either.

### Caching, which is the whole difficulty

ADR-0001 put the read path behind Cloudflare with `s-maxage=3600` because the
pages are the same bytes for every visitor. Negotiation makes that false: one
URL now has two bodies, and Cloudflare does not honour a custom `Vary` for HTML
— the same reason `/search` is `no-store` there.

So:

- Every negotiated response carries `Vary: Accept`, for caches that do honour
  it and because it is what the resource actually varies on.
- The **Markdown** representation is additionally `no-store`. A stored Markdown
  entry under a key that ignores `Accept` would be served to browsers for an
  hour; a browser that has to re-fetch is a cost worth paying to make that
  impossible. That policy is chosen from the *request*, because a revalidated
  Markdown response is a `304` that `EtagLayer` builds from scratch — `ETag` and
  nothing else, no `Content-Type` to recognise it by, and a `3xx` that the page
  policy would otherwise claim.
- The **HTML** representation keeps `public, max-age=0, s-maxage=3600,
  must-revalidate`. The reader-facing cache win of ADR-0001 is untouched.

That leaves the mirror-image hazard, which the origin cannot fix by itself: a
cached *HTML* entry served to an agent that asked for Markdown. Cloudflare must
be told that `Accept` is part of the cache key for the read path.

## Configuring Cloudflare

One addition to the Cache Rule from ADR-0001, in the dashboard:

**Bypass cache when the request asks for Markdown.** In the same rule that makes
`/` and `^/docs` eligible for cache, add a bypass (or a second, higher-priority
rule) matching

```
any(lower(http.request.headers["accept"][*])[*] contains "text/markdown")
```

with *Cache eligibility: Bypass cache*. Without it, an agent hitting a colo that
already holds the HTML page gets HTML no matter what it asked for.

Two details in that expression are not decoration, because the origin parser is
more permissive than a substring match:

- **`lower(…)`.** Cloudflare's `contains` is case-sensitive; media types are
  not (RFC 7231 §3.1.1.1), so `Accept: Text/Markdown` negotiates Markdown at the
  origin. Without the lowercasing it would miss this rule and be answered from
  the HTML cache.
- **`[*]` and `any(…)` rather than `[0]`.** `http.request.headers` holds every
  value of a repeated header, and the origin parser reads all of them for the
  reason RFC 7230 §3.2.2 gives. Matching only `[0]` would disagree with the
  origin about a request a proxy had split across two `Accept` fields.

The rule is a *superset* of what the origin answers in Markdown, which is the
safe direction: a header naming `text/markdown` but ranking HTML above it
bypasses the cache and is then served HTML, costing an origin hit and nothing
else. The reverse — a request the origin would answer in Markdown, or with a
`406`, that this rule fails to match — is what the election and refusal rules
above rule out.

A custom cache key that includes `Accept` is the other shape this can take
(Enterprise), and is strictly better: agents get cached Markdown instead of
always reaching the origin. Bypass is the version available on every plan, and
agent traffic to a docs site is small next to reader traffic.

Verify both directions after deploying:

```bash
curl -sI https://autumn-web.app/docs/getting-started | grep -i 'content-type\|cf-cache-status'
curl -sI -H 'Accept: text/markdown' https://autumn-web.app/docs/getting-started \
  | grep -i 'content-type\|x-markdown-tokens\|cf-cache-status'
```

## Alternatives rejected

**Cloudflare's own Markdown for Agents.** A zone setting, no application code,
and it would have shipped in an afternoon. It converts the rendered HTML back to
Markdown at the edge — a round trip through a document the origin built by
rendering the very Markdown being reconstructed, and one that has to guess at
what the sidebar, the header and the pagination footer were. The origin can hand
over the source instead, and knows which parts of the page are chrome. It also
keeps the behaviour in this repository, testable in `cargo test`, rather than in
a dashboard nobody can diff.

**A parallel `.md` URL space** (`/docs/{slug}.md`). Cacheable without any of the
`Vary` trouble above, since the URL is the cache key. Rejected as the *primary*
mechanism because it is not the convention the skill or Cloudflare describe, and
an agent would have to know to rewrite the URL it was given. It remains an easy
addition if the bypass rule proves too costly.

**Converting the rendered HTML in a middleware layer.** The site would have
carried an HTML-to-Markdown converter to reproduce a string it already has.

## Consequences

- Any HTTP client that sets one header reads these guides as Markdown, without
  MCP, at the URL a human would have sent it.
- `x-markdown-tokens` lets a caller size a document before reading it. It is an
  estimate from body length, deliberately: an exact count is model-specific and
  would cost a tokenizer pass per request.
- **One more Cloudflare rule to configure**, and it is not optional — without
  it, cached pages defeat negotiation at the edge for the most-read guides.
- **`q=0` is honoured only when Markdown is named.** The price of the invariant
  above: `Accept: text/html;q=0` is served the HTML page it refused. No client
  in the wild sends it, and the alternative was a `406` a cached colo would have
  replaced with the same page anyway.
- Relative links inside a guide (`tutorial/index.md`, `../examples/todo-app`)
  are served as authored. The HTML render rewrites them; the Markdown does not,
  because it hands over the source unmodified and because that is already what
  the MCP tools return, so the two agent-facing surfaces agree. Rewriting them
  would mean re-serializing Markdown from parsed events, which is a converter of
  the kind this ADR just declined to add.
- The home page now says all of this to whoever reads it, in both
  representations: an agent that landed on one Markdown page learns the rest of
  the site works the same way.

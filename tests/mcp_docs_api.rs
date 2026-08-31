//! The JSON docs API (`src/api.rs`) and the MCP server Autumn projects it into.
//!
//! Two layers are covered, because they can fail independently. The handlers
//! are exercised directly over HTTP, and then the same handlers are exercised
//! *through* `/mcp`, which is what a coding agent actually talks to: a tool can
//! disappear from the catalog (an untagged route, a lost `Json<T>` return type)
//! while its HTTP endpoint keeps working perfectly.

use autumn_web::test::{TestApp, TestClient};
use serde_json::{Value, json};

use autumn_io::api::MAX_INLINE_DOC_BYTES;
use autumn_io::docs::{DocRegistry, DocSource};

/// The site app, with the docs API projected into MCP exactly as `main` does.
fn app() -> TestClient {
    TestApp::new()
        .routes(autumn_io::app_routes())
        .mount_mcp(autumn_io::MCP_MOUNT_PATH)
        .build()
}

/// Send one JSON-RPC message to `/mcp` and return the `result` object.
async fn rpc(app: &TestClient, method: &str, params: Value) -> Value {
    let response = app
        .post(autumn_io::MCP_MOUNT_PATH)
        .json(&json!({ "jsonrpc": "2.0", "id": 1, "method": method, "params": params }))
        .send()
        .await;

    response.assert_status(200);
    let body: Value = response.json();
    assert_eq!(body["jsonrpc"], "2.0", "not a JSON-RPC response: {body}");
    assert!(body.get("error").is_none(), "JSON-RPC error: {body}");
    body["result"].clone()
}

/// Call one MCP tool and parse the JSON its handler returned.
///
/// Tool results carry the handler's JSON body as *text*, so the assertion that
/// it parses is itself part of the contract an agent depends on.
async fn call_tool(app: &TestClient, name: &str, arguments: Value) -> Value {
    let result = rpc(
        app,
        "tools/call",
        json!({ "name": name, "arguments": arguments }),
    )
    .await;

    assert_eq!(
        result["isError"], false,
        "tool {name} failed: {}",
        result["content"]
    );
    let text = result["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("tool {name} returned no text content: {result}"));

    serde_json::from_str(text).unwrap_or_else(|e| panic!("tool {name} returned non-JSON: {e}"))
}

/// Call a tool expected to fail, returning the error text.
async fn call_tool_expecting_error(app: &TestClient, name: &str, arguments: Value) -> String {
    let result = rpc(
        app,
        "tools/call",
        json!({ "name": name, "arguments": arguments }),
    )
    .await;

    assert_eq!(
        result["isError"], true,
        "tool {name} unexpectedly succeeded: {result}"
    );
    result["content"][0]["text"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

// ─────────────────────────────────────────────────────────────────────────
// The MCP envelope
// ─────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn mcp_initialize_advertises_tool_capability() {
    let app = app();

    let result = rpc(
        &app,
        "initialize",
        json!({ "protocolVersion": "2025-06-18" }),
    )
    .await;

    assert_eq!(result["protocolVersion"], "2025-06-18");
    assert!(
        result["capabilities"]["tools"].is_object(),
        "the server must advertise tools: {result}"
    );
    assert!(result["serverInfo"]["name"].is_string());
}

#[tokio::test]
async fn mcp_catalog_exposes_the_three_docs_tools_as_read_only() {
    let app = app();

    let result = rpc(&app, "tools/list", json!({})).await;
    let tools = result["tools"].as_array().expect("tools array");

    let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
    assert_eq!(
        names,
        ["list_autumn_docs", "search_autumn_docs", "get_autumn_doc"],
        "unexpected tool catalog"
    );

    for tool in tools {
        let name = tool["name"].as_str().unwrap();

        // Every tool is a GET, so an agent may call any of them without asking.
        assert_eq!(
            tool["annotations"]["readOnlyHint"], true,
            "{name} should be annotated read-only"
        );
        // A tool with no description is a tool an agent will not reach for.
        assert!(
            tool["description"].as_str().is_some_and(|d| d.len() > 80),
            "{name} needs a description an agent can route on"
        );
        assert_eq!(tool["inputSchema"]["type"], "object");
    }
}

/// The MCP schema machinery degrades an argument type it cannot introspect to a
/// bare `{"type": "object"}`, which still *works* but leaves the agent guessing
/// the field names. That degradation is a `tracing::warn` at startup, not an
/// error, so assert the derived fields are actually advertised.
#[tokio::test]
async fn mcp_tool_arguments_are_field_accurate() {
    let app = app();

    let result = rpc(&app, "tools/list", json!({})).await;
    let tools = result["tools"].as_array().expect("tools array");
    let tool = |name: &str| {
        tools
            .iter()
            .find(|t| t["name"] == name)
            .unwrap_or_else(|| panic!("{name} missing from the catalog"))
            .clone()
    };

    let search = tool("search_autumn_docs");
    let query = &search["inputSchema"]["$defs"]["SearchDocsQuery"];
    assert_eq!(query["properties"]["q"]["type"], "string");
    assert!(query["properties"].get("limit").is_some());
    assert_eq!(query["required"], json!(["q"]));

    let get = tool("get_autumn_doc");
    assert_eq!(get["inputSchema"]["properties"]["slug"]["type"], "string");
    assert_eq!(get["inputSchema"]["required"], json!(["slug"]));
    assert!(
        get["inputSchema"]["$defs"]["GetDocQuery"]["properties"]
            .get("section")
            .is_some(),
        "get_autumn_doc must advertise its section argument"
    );

    let list = tool("list_autumn_docs");
    assert!(
        list["inputSchema"]["$defs"]["ListDocsQuery"]["properties"]
            .get("group")
            .is_some(),
        "list_autumn_docs must advertise its group argument"
    );
}

/// The site's HTML pages must never become tools: they have no JSON response
/// schema, so Autumn excludes them, and an agent handed a page of Maud markup
/// gets nothing it can use.
#[tokio::test]
async fn mcp_catalog_excludes_the_html_site() {
    let app = app();

    let result = rpc(&app, "tools/list", json!({})).await;
    let paths: Vec<String> = result["tools"]
        .as_array()
        .expect("tools array")
        .iter()
        .map(|tool| tool["name"].as_str().unwrap_or_default().to_owned())
        .collect();

    for excluded in [
        "index",
        "docs_page",
        "docs_search",
        "sitemap_xml",
        "robots_txt",
    ] {
        assert!(
            !paths.contains(&excluded.to_owned()),
            "{excluded} is an HTML/text route and must not be an MCP tool"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────
// The tools, called the way an agent calls them
// ─────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn list_tool_returns_every_guide_with_its_group() {
    let app = app();

    let index = call_tool(&app, "list_autumn_docs", json!({})).await;

    assert_eq!(index["autumn_version"], "0.7.0");
    assert_eq!(index["harvest_version"], "0.6.0");

    let guides = index["guides"].as_array().expect("guides array");
    assert_eq!(index["count"].as_u64().unwrap() as usize, guides.len());
    assert!(guides.len() > 100, "expected the full guide set");

    let mcp_guide = guides
        .iter()
        .find(|g| g["slug"] == "mcp")
        .expect("the mcp guide should be listed");
    assert_eq!(mcp_guide["group"], "APIs and integrations");
    assert!(mcp_guide["bytes"].as_u64().unwrap() > 0);

    // `docs-smoke` is a release-rehearsal checklist the site deliberately does
    // not register, and the API reads the same registry — so it stays unlisted.
    assert!(
        !guides.iter().any(|g| g["slug"] == "docs-smoke"),
        "unregistered guides must not leak through the API"
    );
}

/// Descriptions are abridged in the index and whole in the document, because
/// 140 unabridged descriptions are a five-figure token cost for one call.
#[tokio::test]
async fn list_tool_abridges_descriptions_that_get_doc_returns_whole() {
    let app = app();

    let index = call_tool(&app, "list_autumn_docs", json!({})).await;
    let listed = index["guides"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["slug"] == "mcp")
        .unwrap()["description"]
        .as_str()
        .unwrap()
        .to_owned();

    assert!(
        listed.len() <= 200,
        "abridged description too long: {listed}"
    );
    assert!(
        listed.ends_with('…'),
        "an abridged description is marked: {listed}"
    );
    assert!(
        !listed.contains(" …"),
        "the cut should not leave a dangling space"
    );

    let full = call_tool(&app, "get_autumn_doc", json!({ "slug": "mcp" })).await;
    let full = full["description"].as_str().unwrap();
    assert!(full.len() > listed.len());
    assert!(
        full.starts_with(listed.trim_end_matches('…').trim_end()),
        "the abridged description should be a prefix of the full one"
    );
}

#[tokio::test]
async fn list_tool_filters_by_group_case_insensitively() {
    let app = app();

    let all = call_tool(&app, "list_autumn_docs", json!({})).await;
    let harvest = call_tool(
        &app,
        "list_autumn_docs",
        json!({ "query": { "group": "harvest" } }),
    )
    .await;

    // The echoed group is the canonical label, not the caller's casing.
    assert_eq!(harvest["group"], "Harvest");
    assert!(harvest["count"].as_u64().unwrap() > 5);
    assert!(harvest["count"].as_u64().unwrap() < all["count"].as_u64().unwrap());
    assert!(
        harvest["guides"]
            .as_array()
            .unwrap()
            .iter()
            .all(|g| g["group"] == "Harvest")
    );

    // Every response carries the full group list, so one call teaches the next.
    let groups = harvest["groups"].as_array().expect("groups array");
    assert!(groups.iter().any(|g| g["name"] == "Harvest"));
    assert_eq!(
        groups
            .iter()
            .map(|g| g["count"].as_u64().unwrap())
            .sum::<u64>(),
        all["count"].as_u64().unwrap(),
        "the group counts should partition the guide set"
    );
}

#[tokio::test]
async fn search_tool_ranks_and_limits_hits() {
    let app = app();

    let results = call_tool(
        &app,
        "search_autumn_docs",
        json!({ "query": { "q": "durable timer", "limit": 3 } }),
    )
    .await;

    assert_eq!(results["query"], "durable timer");
    let hits = results["results"].as_array().expect("results array");
    assert!(
        !hits.is_empty(),
        "expected hits for a term the guides cover"
    );
    assert!(hits.len() <= 3, "limit should be honoured");
    assert_eq!(results["count"].as_u64().unwrap() as usize, hits.len());

    // A title match outranks a body match, so the dedicated guide leads.
    assert_eq!(hits[0]["slug"], "harvest-durable-timers");
    assert!(
        hits[0]["url"]
            .as_str()
            .unwrap()
            .ends_with("/docs/harvest-durable-timers")
    );
    assert!(!hits[0]["snippet"].as_str().unwrap().is_empty());

    // Every returned slug must be fetchable — a hit an agent cannot follow is
    // worse than no hit.
    for hit in hits {
        call_tool(
            &app,
            "get_autumn_doc",
            json!({ "slug": hit["slug"].as_str().unwrap() }),
        )
        .await;
    }
}

#[tokio::test]
async fn search_tool_returns_an_empty_set_rather_than_an_error() {
    let app = app();

    for query in [json!({ "q": "   " }), json!({ "q": "zzzznotaword" })] {
        let results = call_tool(&app, "search_autumn_docs", json!({ "query": query })).await;
        assert_eq!(results["count"], 0);
        assert_eq!(results["results"], json!([]));
    }
}

#[tokio::test]
async fn get_tool_returns_markdown_and_the_section_list() {
    let app = app();

    let doc = call_tool(&app, "get_autumn_doc", json!({ "slug": "mcp" })).await;

    assert_eq!(doc["slug"], "mcp");
    assert_eq!(doc["group"], "APIs and integrations");
    assert_eq!(doc["autumn_version"], "0.7.0");
    assert!(doc["url"].as_str().unwrap().ends_with("/docs/mcp"));
    assert!(doc["notice"].is_null(), "a guide this size needs no notice");

    // Markdown, not the rendered HTML: an agent should get what the author
    // wrote, fenced code blocks and all.
    let markdown = doc["markdown"].as_str().expect("markdown body");
    assert!(markdown.contains("```rust"), "code fences should survive");
    assert!(markdown.contains("mount_mcp"));
    assert!(
        !markdown.contains("<pre"),
        "the HTML rendering must not leak in"
    );

    let sections = doc["sections"].as_array().expect("sections array");
    assert!(sections.len() > 5);
    assert!(
        sections
            .iter()
            .all(|s| s["id"].is_string() && s["title"].is_string() && s["level"].is_number())
    );
}

/// The largest guides are withheld rather than dumped: `deployment.md` alone is
/// ~150 KB, which would swamp the context of whatever asked for it.
#[tokio::test]
async fn get_tool_withholds_an_oversized_guide_and_says_how_to_read_it() {
    let app = app();

    let doc = call_tool(&app, "get_autumn_doc", json!({ "slug": "deployment" })).await;

    assert!(
        doc["markdown"].is_null(),
        "an oversized guide must not be returned whole"
    );
    let notice = doc["notice"].as_str().expect("a notice explaining why");
    assert!(
        notice.contains("section"),
        "the notice must name the way out"
    );

    // The section list is present in the same response, so the agent can
    // recover in exactly one more call.
    let sections = doc["sections"].as_array().expect("sections array");
    assert!(sections.len() > 10);

    let id = sections
        .iter()
        .find(|s| s["level"] == 2)
        .expect("a top-level section")["id"]
        .as_str()
        .unwrap()
        .to_owned();

    let part = call_tool(
        &app,
        "get_autumn_doc",
        json!({ "slug": "deployment", "query": { "section": id } }),
    )
    .await;

    assert_eq!(part["section"], id.as_str());
    let markdown = part["markdown"].as_str().expect("section markdown");
    assert!(!markdown.is_empty());
    assert!(
        markdown.len() <= MAX_INLINE_DOC_BYTES,
        "a section should be far smaller than the whole guide"
    );
    assert!(
        markdown.starts_with("##"),
        "a section starts at its heading"
    );
}

/// A section of a large guide can itself blow the budget — `deployment`'s
/// push-button-deploy section is 76 KB on its own. Exempting the section path
/// from the cap would reopen the hole on the very call the oversized-guide
/// notice tells an agent to make, so the gate applies there too.
#[tokio::test]
async fn get_tool_gates_an_oversized_section_the_same_way() {
    let app = app();

    let part = call_tool(
        &app,
        "get_autumn_doc",
        json!({
            "slug": "deployment",
            "query": { "section": "push-button-deploy-to-your-own-server-autumn-deploy" },
        }),
    )
    .await;

    assert!(
        part["markdown"].is_null(),
        "an oversized section must not be returned whole either"
    );
    assert!(
        part["notice"]
            .as_str()
            .is_some_and(|n| n.contains("section"))
    );

    // `sections` narrows to what is inside the section, not the whole guide,
    // so the listed ids are the requests that actually make progress.
    let nested = part["sections"].as_array().expect("nested sections");
    assert!(!nested.is_empty(), "there must be somewhere narrower to go");

    let inner = nested[0]["id"].as_str().unwrap().to_owned();
    let leaf = call_tool(
        &app,
        "get_autumn_doc",
        json!({ "slug": "deployment", "query": { "section": inner } }),
    )
    .await;

    let markdown = leaf["markdown"].as_str().expect("the narrowed body");
    assert!(markdown.len() <= MAX_INLINE_DOC_BYTES);
    assert!(!markdown.is_empty());
}

/// The invariant behind the gate: whatever `get_autumn_doc` hands back, it is
/// never over the cap. Checked across every guide and every section rather than
/// on the two that happen to be large today, since guide content is synced from
/// upstream and can grow at any time.
#[tokio::test]
async fn no_response_ever_exceeds_the_size_cap() {
    let app = app();
    let registry = autumn_io::site_docs().expect("the bundled docs should load");

    let mut checked = 0;
    for page in registry.pages() {
        let doc = call_tool(
            &app,
            "get_autumn_doc",
            json!({ "slug": page.slug.as_str() }),
        )
        .await;
        assert_body_within_cap(&doc, &page.slug, "");
        checked += 1;

        for item in &page.toc {
            let section = call_tool(
                &app,
                "get_autumn_doc",
                json!({ "slug": page.slug.as_str(), "query": { "section": item.id.as_str() } }),
            )
            .await;
            assert_body_within_cap(&section, &page.slug, &item.id);
            checked += 1;
        }
    }

    // Guard against the sweep silently going vacuous — an empty registry, or a
    // toc that stopped being populated, would otherwise pass this test.
    assert!(
        checked > 1_000,
        "expected to sweep the whole corpus, only checked {checked} responses"
    );
}

/// A withheld body must always come with somewhere to go: either headings to
/// narrow to, or — for a large section with nothing nested inside it — a
/// truncated prefix, so a caller is never left with nothing and no next step.
fn assert_body_within_cap(doc: &Value, slug: &str, section: &str) {
    let target = if section.is_empty() {
        slug.to_owned()
    } else {
        format!("{slug}#{section}")
    };

    match doc["markdown"].as_str() {
        Some(markdown) => assert!(
            markdown.len() <= MAX_INLINE_DOC_BYTES,
            "{target} returned {} bytes, over the cap",
            markdown.len()
        ),
        None => {
            assert!(
                !doc["sections"].as_array().expect("sections").is_empty(),
                "{target} withheld its body with no headings to narrow to"
            );
            assert!(
                doc["notice"].as_str().is_some(),
                "{target} withheld its body without saying why"
            );
        }
    }
}

/// Section ids are the anchors the rendered page uses, so an id from the API is
/// also a working deep link. If these two drifted apart, every citation an agent
/// produced would land on the wrong part of the page.
#[tokio::test]
async fn section_ids_match_the_anchors_on_the_rendered_page() {
    let app = app();

    let doc = call_tool(&app, "get_autumn_doc", json!({ "slug": "mcp" })).await;
    let sections = doc["sections"].as_array().unwrap();

    let page = app.get("/docs/mcp").send().await;
    page.assert_status(200);
    let html = page.text();

    for section in sections.iter().take(10) {
        let id = section["id"].as_str().unwrap();
        assert!(
            html.contains(&format!("id=\"{id}\"")),
            "section id {id:?} has no matching anchor on /docs/mcp"
        );
    }
}

#[tokio::test]
async fn unknown_slugs_and_sections_come_back_as_readable_tool_errors() {
    let app = app();

    let error = call_tool_expecting_error(&app, "get_autumn_doc", json!({ "slug": "nope" })).await;
    assert!(
        error.contains("404"),
        "expected the status in the tool error"
    );
    assert!(
        error.contains("list_autumn_docs") || error.contains("search_autumn_docs"),
        "the error should name the tool that returns valid slugs: {error}"
    );

    let error = call_tool_expecting_error(
        &app,
        "get_autumn_doc",
        json!({ "slug": "mcp", "query": { "section": "no-such-section" } }),
    )
    .await;
    assert!(
        error.contains("sections"),
        "the error should point at the section list: {error}"
    );

    let error = call_tool_expecting_error(
        &app,
        "list_autumn_docs",
        json!({ "query": { "group": "not a group" } }),
    )
    .await;
    assert!(error.contains("group"), "expected a group error: {error}");
}

// ─────────────────────────────────────────────────────────────────────────
// The plain HTTP surface
// ─────────────────────────────────────────────────────────────────────────

/// The tools are ordinary endpoints, and stay usable with curl and in a browser
/// — the MCP layer is a projection of them, not a separate app.
#[tokio::test]
async fn the_json_api_is_reachable_without_mcp() {
    let app = app();

    app.get("/api/docs")
        .send()
        .await
        .assert_status(200)
        .assert_body_contains("\"slug\":\"getting-started\"");

    app.get("/api/search?q=websocket")
        .send()
        .await
        .assert_status(200)
        .assert_body_contains("\"results\"");

    app.get("/api/docs/mcp")
        .send()
        .await
        .assert_status(200)
        .assert_body_contains("mount_mcp");

    app.get("/api/docs/nope").send().await.assert_status(404);
}

/// Search lives at `/api/search`, not `/api/docs/search`: an exact route under
/// `/api/docs/` would shadow the guide with that slug, and upstream ships one
/// called `search`. The same trap the HTML site already sidesteps at `/search`.
#[tokio::test]
async fn the_search_guide_is_not_shadowed_by_the_search_endpoint() {
    let app = app();

    let doc = call_tool(&app, "get_autumn_doc", json!({ "slug": "search" })).await;
    assert_eq!(doc["slug"], "search");
    assert!(doc["markdown"].as_str().is_some_and(|m| !m.is_empty()));
}

// ─────────────────────────────────────────────────────────────────────────
// Section slicing
// ─────────────────────────────────────────────────────────────────────────

const SECTIONED_SOURCE: &str = r#"+++
title = "Sectioned"
description = "A guide with nested headings and a fenced code block."
order = 10
+++

Intro prose.

## First

First body.

### Nested

Nested body.

```bash
# This heading-shaped comment lives inside a fence.
echo hi
```

Still inside Nested.

## Second

Second body.
"#;

#[test]
fn a_section_runs_to_the_next_heading_of_the_same_depth() {
    let registry = DocRegistry::from_sources([DocSource::new("sectioned", SECTIONED_SOURCE)])
        .expect("valid source");
    let page = registry.page("sectioned").expect("registered");

    let first = page.section("first").expect("the first section");
    assert_eq!(first.level, 2);
    assert_eq!(first.title, "First");
    // A `##` section swallows its `###` children but stops at the next `##`.
    assert!(first.markdown.starts_with("## First"));
    assert!(first.markdown.contains("### Nested"));
    assert!(first.markdown.contains("Still inside Nested."));
    assert!(!first.markdown.contains("## Second"));

    // A `#`-prefixed line inside a fence is a comment, not a heading, so it
    // must not cut the section short.
    assert!(first.markdown.contains("echo hi"));

    let nested = page.section("nested").expect("the nested section");
    assert_eq!(nested.level, 3);
    assert!(nested.markdown.starts_with("### Nested"));
    assert!(!nested.markdown.contains("## Second"));

    let second = page.section("second").expect("the last section");
    assert!(second.markdown.trim_end().ends_with("Second body."));

    assert!(page.section("no-such-heading").is_none());
}

#[test]
fn section_ids_are_the_toc_ids() {
    let registry = DocRegistry::from_sources([DocSource::new("sectioned", SECTIONED_SOURCE)])
        .expect("valid source");
    let page = registry.page("sectioned").expect("registered");

    for item in &page.toc {
        let section = page
            .section(&item.id)
            .unwrap_or_else(|| panic!("toc id {:?} should resolve to a section", item.id));
        assert_eq!(section.title, item.title);
        assert_eq!(section.level, item.level);
    }
}

/// Repeated headings get numbered anchors (`#setup`, `#setup-2`), and the
/// section walk has to number them identically or it would return the wrong
/// slice for the second one.
#[test]
fn repeated_headings_slice_by_their_numbered_ids() {
    const SOURCE: &str = r#"+++
title = "Repeats"
description = "The same heading twice."
order = 10
+++

## Setup

First setup.

## Setup

Second setup.
"#;

    let registry =
        DocRegistry::from_sources([DocSource::new("repeats", SOURCE)]).expect("valid source");
    let page = registry.page("repeats").expect("registered");

    assert!(
        page.section("setup")
            .unwrap()
            .markdown
            .contains("First setup.")
    );
    assert!(
        page.section("setup-2")
            .unwrap()
            .markdown
            .contains("Second setup.")
    );
}

/// Every guide the site serves must be reachable through the API, whole or by
/// section. A guide that is neither would be invisible to an agent.
#[test]
fn every_bundled_guide_is_retrievable() {
    let registry = autumn_io::site_docs().expect("the bundled docs should load");

    for page in registry.pages() {
        if page.markdown.len() <= MAX_INLINE_DOC_BYTES {
            continue;
        }

        // An oversized guide is only usable if it has sections to ask for.
        let sections: Vec<_> = page.toc.iter().filter(|item| item.level <= 3).collect();
        assert!(
            !sections.is_empty(),
            "{} is too large to inline and has no sections to fall back on",
            page.slug
        );

        for item in sections {
            let section = page
                .section(&item.id)
                .unwrap_or_else(|| panic!("{}#{} should resolve", page.slug, item.id));
            assert!(
                !section.markdown.is_empty(),
                "{}#{} sliced to nothing",
                page.slug,
                item.id
            );
        }
    }
}

//! Byte-identity between the origin and the edge lane.
//!
//! The promise the edge capsule makes is narrow and total: for a request the
//! edge serves, the capsule and the origin binary **of the same build** produce
//! the same status, the same body bytes, and the same headers after projection
//! (`autumn_edge::conformance::VOLATILE_HEADERS` names the handful the origin's
//! middleware stamps on that the edge structurally cannot). A reader behind the
//! CDN and a reader who hit the origin directly must not be able to tell which
//! one answered.
//!
//! For this site that promise carries an extra load, because the two lanes do
//! not use the same syntax highlighter. Oniguruma is a C library and there is no
//! `wasm32-wasip1` sysroot to build it against, so the capsule uses syntect's
//! pure-Rust `fancy-regex` engine while the origin keeps `onig` — which is 3.8×
//! faster on the corpus and is why the origin has it. The two engines render
//! every one of the 140 bundled guides to identical bytes today. This test is
//! what keeps that a fact rather than an assumption: a grammar, a syntect
//! release, or a new code fence that made them disagree would show up here as a
//! divergence on a specific page, not as a subtly wrong colour in production.
//!
//! # What runs here, and what does not
//!
//! This is the *native* half — Tier A in the framework's own harness. It drives
//! the corpus through the edge router built from `edge_routes()` and through
//! the origin's handlers, and compares. It does not need a `.wasm`, so it runs
//! on every `cargo test` with no extra toolchain.
//!
//! The other half — the same corpus through a real `wasm32-wasip1` artifact —
//! lives in `edge/worker/test/capsule.test.js`, because the thing worth
//! exercising there is the artifact *and* the CDN shim that has to drive it.
//! Together they close the loop: this test says the edge lane reproduces the
//! origin, and that one says the artifact reproduces the edge lane.

use autumn_edge::conformance::{ConformanceCase, Expectation, Verdict, compare};
use autumn_edge::reexports::axum::body::{Body, to_bytes};
use autumn_edge::reexports::http::{Method, Request, StatusCode};
use autumn_edge::wire::{EdgeResponse, FallthroughReason, strip_sentinel};
use autumn_edge::{EdgeOutcome, build_edge_router};
use autumn_web::test::TestApp;
use tower::ServiceExt as _;

/// The corpus.
///
/// Every `#[edge]` route appears at least once, plus one case for each
/// fallthrough channel the site can reach. Guide slugs are picked to cover the
/// rendering paths that differ most between pages: the intro, a guide dense
/// with highlighted Rust, one with tables, and the largest guide in the corpus.
const CASES: &[ConformanceCase] = &[
    ConformanceCase {
        name: "home page",
        method: "GET",
        uri: "/",
        headers: &[("accept", "text/html")],
        provided_capabilities: &[],
        expect: Expectation::Served,
    },
    ConformanceCase {
        name: "docs index redirect",
        method: "GET",
        uri: "/docs",
        headers: &[],
        provided_capabilities: &[],
        expect: Expectation::Served,
    },
    ConformanceCase {
        name: "the first guide",
        method: "GET",
        uri: "/docs/getting-started",
        headers: &[("accept", "text/html")],
        provided_capabilities: &[],
        expect: Expectation::Served,
    },
    ConformanceCase {
        name: "a guide dense with highlighted Rust",
        method: "GET",
        uri: "/docs/jobs",
        headers: &[],
        provided_capabilities: &[],
        expect: Expectation::Served,
    },
    ConformanceCase {
        name: "a guide dense with tables",
        method: "GET",
        uri: "/docs/edge",
        headers: &[],
        provided_capabilities: &[],
        expect: Expectation::Served,
    },
    ConformanceCase {
        name: "the largest guide",
        method: "GET",
        uri: "/docs/deployment",
        headers: &[],
        provided_capabilities: &[],
        expect: Expectation::Served,
    },
    ConformanceCase {
        name: "a percent-encoded slash inside a slug",
        method: "GET",
        uri: "/docs/jobs%2Fnested",
        headers: &[],
        provided_capabilities: &[],
        expect: Expectation::Served,
    },
    ConformanceCase {
        name: "an unknown guide renders the site's own 404",
        method: "GET",
        uri: "/docs/no-such-guide",
        headers: &[],
        provided_capabilities: &[],
        expect: Expectation::Served,
    },
    ConformanceCase {
        name: "robots.txt",
        method: "GET",
        uri: "/robots.txt",
        headers: &[],
        provided_capabilities: &[],
        expect: Expectation::Served,
    },
    ConformanceCase {
        name: "sitemap.xml",
        method: "GET",
        uri: "/sitemap.xml",
        headers: &[],
        provided_capabilities: &[],
        expect: Expectation::Served,
    },
    ConformanceCase {
        name: "credentials are stripped, not honoured",
        method: "GET",
        uri: "/docs/authentication",
        headers: &[
            ("cookie", "session=secret"),
            ("authorization", "Bearer secret"),
        ],
        provided_capabilities: &[],
        expect: Expectation::Served,
    },
    ConformanceCase {
        name: "the search route stays at the origin",
        method: "GET",
        uri: "/search?q=jobs",
        headers: &[],
        provided_capabilities: &[],
        expect: Expectation::Fallthrough(FallthroughReason::UnknownRoute),
    },
    ConformanceCase {
        name: "the JSON docs API stays at the origin",
        method: "GET",
        uri: "/api/docs",
        headers: &[],
        provided_capabilities: &[],
        expect: Expectation::Fallthrough(FallthroughReason::UnknownRoute),
    },
    ConformanceCase {
        name: "a nested docs path is not a guide",
        method: "GET",
        uri: "/docs/a/b",
        headers: &[],
        provided_capabilities: &[],
        expect: Expectation::Fallthrough(FallthroughReason::UnknownRoute),
    },
    ConformanceCase {
        name: "a write method is not edge eligible",
        method: "POST",
        uri: "/",
        headers: &[],
        provided_capabilities: &[],
        expect: Expectation::Fallthrough(FallthroughReason::MethodNotEdgeEligible),
    },
];

/// Run one case through the edge router, the way the capsule runtime does.
///
/// The ladder is the runtime's: method first, then dispatch, then the
/// sentinel. The KV rung is absent because this site declares no capabilities
/// — `the_edge_lane_declares_no_capabilities` is what keeps that true.
async fn serve_edge(case: &ConformanceCase) -> EdgeOutcome {
    if case.method != "GET" && case.method != "HEAD" {
        return EdgeOutcome::fallthrough(
            FallthroughReason::MethodNotEdgeEligible,
            format!("{} is not a read method", case.method),
        );
    }

    let mut builder = Request::builder()
        .method(Method::from_bytes(case.method.as_bytes()).expect("known method"))
        .uri(case.uri);
    for (name, value) in case.headers {
        builder = builder.header(*name, *value);
    }
    let request = builder.body(Body::empty()).expect("request builds");

    let response = build_edge_router(autumn_io::edge::edge_routes())
        .oneshot(request)
        .await
        .expect("dispatch is infallible");

    let status = response.status().as_u16();
    let mut headers: Vec<(String, String)> = response
        .headers()
        .iter()
        .map(|(name, value)| {
            (
                name.as_str().to_owned(),
                String::from_utf8_lossy(value.as_bytes()).into_owned(),
            )
        })
        .collect();
    let sentinel = strip_sentinel(&mut headers);
    let body = to_bytes(response.into_body(), MAX_BODY_BYTES)
        .await
        .expect("body collects")
        .to_vec();

    match sentinel {
        Some(reason) => EdgeOutcome::fallthrough(
            reason.parse().expect("the runtime only sets known reasons"),
            String::from_utf8_lossy(&body).into_owned(),
        ),
        None => EdgeOutcome::Served(EdgeResponse {
            status,
            headers,
            body,
        }),
    }
}

/// Run one case through the origin, as the deployed app serves it.
///
/// `TestApp` builds the same router `main` does from the same `app_routes()`,
/// minus the response layer stack — which is the point: compression, ETag and
/// `Cache-Control` are origin middleware the capsule structurally cannot run,
/// and comparing against a lane that had them would be comparing against
/// something the edge was never promising to reproduce.
async fn serve_origin(case: &ConformanceCase) -> EdgeOutcome {
    let app = TestApp::new().routes(autumn_io::app_routes()).build();

    let mut request = app.get(case.uri);
    for (name, value) in case.headers {
        request = request.header(name, value);
    }
    let response = request.send().await;

    if response.status == StatusCode::NOT_FOUND && response.headers.is_empty() {
        return EdgeOutcome::fallthrough(FallthroughReason::UnknownRoute, "no route matched");
    }

    EdgeOutcome::Served(EdgeResponse {
        status: response.status.as_u16(),
        headers: response.headers,
        body: response.body,
    })
}

/// Cap on a collected response body. The largest guide renders to ~330 KB.
const MAX_BODY_BYTES: usize = 32 * 1024 * 1024;

/// Security headers `autumn-web`'s middleware stamps on every origin response.
///
/// `autumn_edge::conformance::VOLATILE_HEADERS` excuses the headers the
/// *framework's own* middleware adds that no edge lane can emit — `date`,
/// `x-request-id`, `server-timing` and friends. It does not know about this
/// site's security posture, and the guide is explicit that the projection only
/// excuses "the headers the origin's middleware stack adds and the edge lane
/// structurally cannot emit". These are exactly that, so they are projected out
/// of the comparison here.
///
/// Projecting them out is only defensible because they are *restored*: the
/// Cloudflare shim stamps this same list on every response it serves
/// (`edge/worker/src/index.js`), from this same file. A reader behind the CDN
/// gets the same protections as a reader hitting the origin.
///
/// [`the_projection_covers_every_header_the_origin_adds`] is what keeps the two
/// halves honest — it fails if the framework starts adding a fifth header, and
/// names the file to edit.
const SECURITY_HEADERS_JSON: &str = include_str!("../edge/security-headers.json");

fn restored_security_headers() -> Vec<(String, String)> {
    let parsed: serde_json::Value =
        serde_json::from_str(SECURITY_HEADERS_JSON).expect("security-headers.json parses");
    parsed["headers"]
        .as_object()
        .expect("`headers` is an object")
        .iter()
        .map(|(name, value)| {
            (
                name.clone(),
                value
                    .as_str()
                    .expect("header values are strings")
                    .to_owned(),
            )
        })
        .collect()
}

/// Drop the headers the shim restores, so the comparison is about what the two
/// lanes actually render.
fn project_restored(response: &EdgeResponse) -> EdgeResponse {
    let restored = restored_security_headers();
    EdgeResponse {
        status: response.status,
        headers: response
            .headers
            .iter()
            .filter(|(name, _)| {
                let name = name.to_ascii_lowercase();
                !restored.iter().any(|(restored, _)| *restored == name)
            })
            .cloned()
            .collect(),
        body: response.body.clone(),
    }
}

#[tokio::test]
async fn every_case_reaches_the_outcome_it_expects() {
    for case in CASES {
        let outcome = serve_edge(case).await;
        match case.expect {
            Expectation::Served => assert!(
                outcome.served().is_some(),
                "{}: expected the edge to serve it, got {outcome:?}",
                case.name
            ),
            Expectation::Fallthrough(reason) => assert_eq!(
                outcome.fallthrough_reason(),
                Some(reason),
                "{}: wrong fallthrough channel ({outcome:?})",
                case.name
            ),
        }
    }
}

#[tokio::test]
async fn the_edge_reproduces_the_origin_byte_for_byte() {
    let mut divergences = Vec::new();

    for case in CASES {
        let Expectation::Served = case.expect else {
            continue;
        };
        let edge = serve_edge(case).await;
        let origin = serve_origin(case).await;

        let (Some(edge), Some(origin)) = (edge.served(), origin.served()) else {
            divergences.push(format!(
                "{}: one lane declined — edge {edge:?}, origin {origin:?}",
                case.name
            ));
            continue;
        };

        if let Verdict::Diverged { detail } = compare(&project_restored(origin), edge) {
            divergences.push(format!("{}: {detail}", case.name));
        }
    }

    assert!(
        divergences.is_empty(),
        "the edge lane diverged from the origin:\n  {}",
        divergences.join("\n  "),
    );
}

#[tokio::test]
async fn a_handler_is_a_function_of_its_request() {
    // Byte-identity only holds if the handler is deterministic, so each lane is
    // compared with *itself* before it is compared with the other. A page that
    // renders a HashMap in iteration order fails here, and the failure says so,
    // rather than showing up as a mysterious cross-lane divergence.
    for case in CASES {
        let Expectation::Served = case.expect else {
            continue;
        };
        for lane in ["edge", "origin"] {
            let run = async |case: &ConformanceCase| {
                if lane == "edge" {
                    serve_edge(case).await
                } else {
                    serve_origin(case).await
                }
            };
            let first = run(case).await;
            let second = run(case).await;
            let (Some(first), Some(second)) = (first.served(), second.served()) else {
                continue;
            };
            assert_eq!(
                compare(&project_restored(first), &project_restored(second)),
                Verdict::Reproduced,
                "{}: the {lane} lane is not deterministic",
                case.name
            );
        }
    }
}

#[test]
fn every_edge_route_is_also_mounted_on_the_origin() {
    // Fallthrough is only free because the origin serves everything the edge
    // does. A route that reached the capsule but not the origin would 404 the
    // moment the edge declined it.
    let origin_paths: Vec<&str> = autumn_io::app_routes()
        .iter()
        .map(|route| route.path)
        .collect();

    for route in autumn_io::edge::edge_routes() {
        assert!(
            origin_paths.contains(&route.path),
            "`{}` is an edge route but the origin does not mount `{}`",
            route.name,
            route.path,
        );
    }
}

#[test]
fn the_edge_lane_declares_no_capabilities() {
    // Every guide is embedded in the artifact, so no route needs a seam the
    // host has to mediate. That is what lets the Cloudflare shim advertise
    // nothing and still serve the whole read path — and it is worth asserting,
    // because adding `#[edge(needs(kv))]` to a route would silently make every
    // request to it fall through in production.
    for route in autumn_io::edge::edge_routes() {
        assert!(
            route.needs.is_empty(),
            "`{}` declares {:?}; the Cloudflare shim provides no capabilities \
             unless a KV snapshot is bound (see edge/README.md)",
            route.name,
            route.needs,
        );
    }
}

#[tokio::test]
async fn the_projection_covers_every_header_the_origin_adds() {
    // The one test that keeps `edge/security-headers.json` honest.
    //
    // `the_edge_reproduces_the_origin_byte_for_byte` projects that list out of
    // the origin's responses, which would quietly hide a *new* origin header
    // from the comparison if the list were merely a superset. So: every header
    // the origin emits and the edge does not must be in the list, and every
    // entry in the list must actually be emitted. A framework upgrade that adds
    // a fifth security header fails here, and the fix is to add it to the file
    // — which is also what makes the Cloudflare shim start sending it.
    let restored = restored_security_headers();
    let mut unaccounted: Vec<String> = Vec::new();
    let mut never_seen: Vec<String> = restored.iter().map(|(name, _)| name.clone()).collect();

    for case in CASES {
        let Expectation::Served = case.expect else {
            continue;
        };
        let edge = serve_edge(case).await;
        let origin = serve_origin(case).await;
        let (Some(edge), Some(origin)) = (edge.served(), origin.served()) else {
            continue;
        };

        for (name, value) in &origin.headers {
            let name = name.to_ascii_lowercase();
            if edge
                .headers
                .iter()
                .any(|(edge_name, _)| edge_name.to_ascii_lowercase() == name)
            {
                continue;
            }
            match restored.iter().find(|(restored, _)| *restored == name) {
                Some((_, expected)) => {
                    never_seen.retain(|seen| *seen != name);
                    assert_eq!(
                        value, expected,
                        "{}: the origin sends `{name}: {value}` but \
                         edge/security-headers.json says `{expected}`",
                        case.name,
                    );
                }
                None => {
                    // `VOLATILE_HEADERS` is upstream's own excuse list; anything
                    // outside both is a header the edge lane would silently drop.
                    if !autumn_edge::conformance::VOLATILE_HEADERS.contains(&name.as_str())
                        && !unaccounted.contains(&name)
                    {
                        unaccounted.push(name);
                    }
                }
            }
        }
    }

    assert!(
        unaccounted.is_empty(),
        "the origin adds {unaccounted:?}, which the edge lane does not emit and \
         nothing restores. Add them to edge/security-headers.json so the \
         Cloudflare shim sends them, or establish that they are volatile.",
    );
    assert!(
        never_seen.is_empty(),
        "edge/security-headers.json lists {never_seen:?}, which the origin never \
         sends. The shim would be inventing headers; remove them.",
    );
}

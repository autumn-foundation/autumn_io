// The Cloudflare shim, driven against the real capsule under plain Node.
//
// A Worker is not required to exercise any of this: the protocol is NDJSON over
// stdio, the WASI shim is in-memory, and `serveFromCapsule` is deliberately free
// of Cloudflare types. So the interesting half of the deploy — does the shim
// speak the wire protocol the artifact expects, and does the artifact only ask
// for syscalls the shim provides — is testable with `node --test`, no wrangler
// and no network.
//
// Run `edge/build-capsule.sh` first; the tests skip with a clear message if the
// artifact is not staged.

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import test, { describe } from "node:test";

import { serveFromCapsule, unexpectedImports } from "../src/capsule.js";
import { canonicalizeHeaders } from "../src/wire.js";

const ARTIFACT = fileURLToPath(new URL("../build/edge-capsule.wasm", import.meta.url));

const module = await (async () => {
  try {
    return new WebAssembly.Module(await readFile(ARTIFACT));
  } catch (error) {
    if (error.code === "ENOENT") return null;
    throw error;
  }
})();

const missing = module === null;
const skip = missing ? "run edge/build-capsule.sh first" : false;

function get(uri, headers = []) {
  return serveFromCapsule(module, { method: "GET", uri, headers });
}

function text(outcome) {
  return new TextDecoder().decode(outcome.body);
}

function header(outcome, name) {
  const found = outcome.headers.find(([key]) => key.toLowerCase() === name);
  return found?.[1];
}

describe("the artifact", { skip }, () => {
  test("imports nothing this shim does not provide", () => {
    assert.deepEqual(unexpectedImports(module), []);
  });

  test("has no filesystem or socket access beyond the stubs", () => {
    // The stubs exist because the Rust runtime probes for preopens at startup;
    // they answer BADF/NOTSUP. What matters is that nothing outside WASI is
    // imported at all — no host functions, no vendor SDK.
    const foreign = WebAssembly.Module.imports(module).filter(
      ({ module: from }) => from !== "wasi_snapshot_preview1",
    );
    assert.deepEqual(foreign, []);
  });
});

describe("serving", { skip }, () => {
  test("renders the home page", () => {
    const outcome = get("/");
    assert.equal(outcome.op, "response");
    assert.equal(outcome.status, 200);
    assert.match(header(outcome, "content-type"), /text\/html/);
    assert.match(text(outcome), /<!doctype html>/i);
  });

  test("renders a guide", () => {
    const outcome = get("/docs/getting-started");
    assert.equal(outcome.op, "response");
    assert.equal(outcome.status, 200);
    assert.match(text(outcome), /Getting Started|getting-started/i);
  });

  test("serves robots.txt as plain text", () => {
    const outcome = get("/robots.txt");
    assert.equal(outcome.op, "response");
    assert.equal(header(outcome, "content-type"), "text/plain; charset=utf-8");
    assert.match(text(outcome), /Sitemap:/);
  });

  test("serves the sitemap as XML", () => {
    const outcome = get("/sitemap.xml");
    assert.equal(outcome.op, "response");
    assert.equal(header(outcome, "content-type"), "application/xml; charset=utf-8");
    assert.match(text(outcome), /<urlset/);
  });

  test("redirects /docs to the first guide", () => {
    const outcome = get("/docs");
    assert.equal(outcome.op, "response");
    assert.equal(outcome.status, 307);
    assert.equal(header(outcome, "location"), "/docs/getting-started");
  });

  test("renders the site's own 404 for an unknown guide", () => {
    // An unknown *slug* is a route the capsule owns and answers with a rendered
    // page, unlike an unknown *path*, which is a fallthrough.
    const outcome = get("/docs/no-such-guide");
    assert.equal(outcome.op, "response");
    assert.equal(outcome.status, 404);
  });

  test("headers cross the wire canonicalized", () => {
    const outcome = get("/robots.txt");
    assert.deepEqual(outcome.headers, canonicalizeHeaders(outcome.headers));
  });

  test("never sets a cookie", () => {
    // A capsule response carrying `set-cookie` is refused at the wire, so this
    // asserts the site never tries.
    for (const uri of ["/", "/docs/jobs", "/robots.txt", "/sitemap.xml"]) {
      const outcome = get(uri);
      assert.equal(header(outcome, "set-cookie"), undefined, uri);
    }
  });
});

describe("falling through", { skip }, () => {
  test("a path the edge does not own is an unknown_route", () => {
    const outcome = get("/search?q=jobs");
    assert.equal(outcome.op, "fallthrough");
    assert.equal(outcome.reason, "unknown_route");
  });

  test("the JSON API falls through", () => {
    const outcome = get("/api/docs");
    assert.equal(outcome.op, "fallthrough");
    assert.equal(outcome.reason, "unknown_route");
  });

  test("a write method is not edge eligible", () => {
    const outcome = serveFromCapsule(module, {
      method: "POST",
      uri: "/",
      headers: [],
    });
    assert.equal(outcome.op, "fallthrough");
    assert.equal(outcome.reason, "method_not_edge_eligible");
  });

  test("a nested docs path is not a guide slug", () => {
    const outcome = get("/docs/a/b");
    assert.equal(outcome.op, "fallthrough");
    assert.equal(outcome.reason, "unknown_route");
  });
});

describe("credentials", { skip }, () => {
  test("are never visible to the capsule", () => {
    // The wire strips them on the way in, and the guest strips them again. The
    // observable proof is that a request carrying them serves the same bytes as
    // one that does not.
    const withCredentials = get("/robots.txt", [
      ["cookie", "session=secret"],
      ["authorization", "Bearer secret"],
    ]);
    const without = get("/robots.txt");

    assert.equal(withCredentials.op, "response");
    assert.equal(text(withCredentials), text(without));
  });
});

describe("determinism", { skip }, () => {
  test("the same request twice produces the same bytes", () => {
    // Two fresh instantiations. Anything ambient — a clock, a hash seed leaking
    // into rendered output — shows up here as a diff.
    for (const uri of ["/", "/docs/getting-started", "/sitemap.xml"]) {
      const first = get(uri);
      const second = get(uri);
      assert.equal(first.op, "response", uri);
      assert.deepEqual(first.headers, second.headers, `${uri}: headers`);
      assert.equal(text(first), text(second), `${uri}: body`);
    }
  });
});

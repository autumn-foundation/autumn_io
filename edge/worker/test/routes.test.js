// The Worker's path pre-filter, pinned against the capsule's real route table.
//
// `isCapsulePath` exists to avoid instantiating a 5 MB module to be told
// "unknown_route" for every `/static/` asset and every form post. It is a
// shortcut, and a shortcut that disagrees with the router it is shortcutting is
// a bug in one of two directions:
//
//   too permissive  → a wasted instantiation, then a correct fallthrough
//   too restrictive → a page the edge could have served goes to the origin
//
// Only the second is silent, which is why the test drives the same paths
// through both and compares.

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import test, { describe } from "node:test";

import { serveFromCapsule } from "../src/capsule.js";

const ARTIFACT = fileURLToPath(new URL("../build/edge-capsule.wasm", import.meta.url));

const module = await (async () => {
  try {
    return new WebAssembly.Module(await readFile(ARTIFACT));
  } catch (error) {
    if (error.code === "ENOENT") return null;
    throw error;
  }
})();
const skip = module === null ? "run edge/build-capsule.sh first" : false;

// A copy of the Worker's pre-filter. `index.js` imports `../build/*.wasm`,
// which only a bundler can resolve, so the function is mirrored here rather
// than imported. `matches_the_worker_filter` below is what keeps the copy
// honest.
function isCapsulePath(pathname) {
  if (pathname === "/" || pathname === "/docs") return true;
  if (pathname === "/robots.txt" || pathname === "/sitemap.xml") return true;
  // `/docs/{slug}` — exactly one non-empty segment. `matchit` does not match an
  // empty capture, so `/docs/` is not a guide.
  const slug = pathname.startsWith("/docs/") ? pathname.slice(6) : "";
  return slug !== "" && !slug.includes("/");
}

const PATHS = [
  "/",
  "/docs",
  "/docs/getting-started",
  "/docs/edge",
  "/docs/no-such-guide",
  "/robots.txt",
  "/sitemap.xml",
  "/search",
  "/api/docs",
  "/api/docs/jobs",
  "/mcp",
  "/health",
  "/_stories",
  "/static/css/autumn.css",
  "/docs/a/b",
  "/docs/",
  "/nope",
];

describe("the Worker path filter", { skip }, () => {
  test("agrees with the capsule's router on every path", () => {
    for (const pathname of PATHS) {
      const outcome = serveFromCapsule(module, {
        method: "GET",
        uri: pathname,
        headers: [],
      });
      const capsuleOwnsIt = outcome.op === "response";
      assert.equal(
        isCapsulePath(pathname),
        capsuleOwnsIt,
        `${pathname}: filter said ${isCapsulePath(pathname)}, capsule said ${capsuleOwnsIt}` +
          (outcome.op === "fallthrough" ? ` (${outcome.reason})` : ""),
      );
    }
  });

  test("matches the copy in the Worker source", async () => {
    // The two definitions are textually identical; if `index.js` is edited
    // without editing this file, this fails.
    const source = await readFile(
      fileURLToPath(new URL("../src/index.js", import.meta.url)),
      "utf8",
    );
    const start = source.indexOf("function isCapsulePath");
    assert.notEqual(start, -1, "the Worker still defines isCapsulePath");
    const rest = source.slice(start);
    const body = rest.slice(0, rest.indexOf("\n}\n") + 2);

    // Compare intent, not layout: comments and whitespace may differ between a
    // module-level definition and `Function.prototype.toString`.
    const normalize = (text) =>
      text
        .replace(/\/\/.*$/gm, "")
        .replace(/\s+/g, " ")
        .trim();
    assert.equal(normalize(body), normalize(isCapsulePath.toString()));
  });
});

describe("restored security headers", () => {
  test("the Worker stamps exactly the shared list", async () => {
    // The origin's middleware stack does not run in a capsule, so these are
    // added by the shim. `tests/edge_conformance.rs` asserts the origin emits
    // exactly this list; this asserts the Worker sends it. Between them the
    // edge lane cannot end up serving weaker headers than the origin.
    const shared = JSON.parse(
      await readFile(fileURLToPath(new URL("../../security-headers.json", import.meta.url)), "utf8"),
    );
    const source = await readFile(
      fileURLToPath(new URL("../src/index.js", import.meta.url)),
      "utf8",
    );

    assert.deepEqual(Object.keys(shared.headers).sort(), [
      "referrer-policy",
      "x-content-type-options",
      "x-frame-options",
      "x-xss-protection",
    ]);
    assert.match(
      source,
      /for \(const \[name, value\] of Object\.entries\(SECURITY_HEADERS\.headers\)\)/,
      "the Worker must stamp every entry of the shared list, not a hand-written subset",
    );
  });
});

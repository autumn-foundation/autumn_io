// Colo cache policy.
//
// Every behaviour pinned here was a bug at some point in this Worker's short
// life, and none of them is visible from a single request: the versioned key
// needs two deployments to go wrong, the HEAD corruption needs a HEAD followed
// by a GET, and the leaked storage TTL needs a cache hit. They are exactly the
// cases a unit test is for.

import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import test, { describe } from "node:test";

import {
  STORED_CACHE_CONTROL,
  bodilessForHead,
  capsuleCacheKey,
  isCacheable,
  storedCopy,
  withoutStorageTtl,
} from "../src/cache.js";

describe("the cache key", () => {
  test("changes when the capsule does", () => {
    // The bug: `caches.default` survives deployments, so a URL-only key let a
    // warmed colo serve the previous deploy's HTML until it expired.
    const url = new URL("https://autumn-web.app/docs/jobs");
    const before = capsuleCacheKey(url, "aaaaaaaaaaaaaaaa");
    const after = capsuleCacheKey(url, "bbbbbbbbbbbbbbbb");

    assert.notEqual(before.url, after.url);
  });

  test("is stable for the same URL and version", () => {
    const url = new URL("https://autumn-web.app/docs/jobs");
    assert.equal(
      capsuleCacheKey(url, "v1").url,
      capsuleCacheKey(new URL(url), "v1").url,
    );
  });

  test("keeps distinct paths and queries distinct", () => {
    const key = (href) => capsuleCacheKey(new URL(href), "v1").url;

    assert.notEqual(key("https://a.test/docs/jobs"), key("https://a.test/docs/tasks"));
    assert.notEqual(key("https://a.test/?a=1"), key("https://a.test/?a=2"));
  });

  test("is always a GET, whatever the request was", () => {
    // Cloudflare's Cache API accepts GET only, for `match` as well as `put`.
    // Keying on the inbound request meant every HEAD missed the cache and its
    // `put` rejected inside `waitUntil`.
    const key = capsuleCacheKey(new URL("https://a.test/docs/jobs"), "v1");
    assert.equal(key.method, "GET");
  });
});

describe("what may be cached", () => {
  const response = (status, headers = {}) => new Response("body", { status, headers });

  test("a HEAD response is never stored", () => {
    // Sharper than an API restriction: axum routes HEAD to the GET handler and
    // strips the body, so the capsule answers a HEAD with zero body bytes and
    // the GET's `content-length`. Stored under a key a GET later reads, that is
    // a truncated page claiming to be the whole one.
    assert.equal(isCacheable("HEAD", response(200)), false);
  });

  test("successes and the docs 404 are stored", () => {
    assert.equal(isCacheable("GET", response(200)), true);
    assert.equal(isCacheable("GET", response(404)), true);
  });

  test("redirects and errors are not", () => {
    for (const status of [307, 500, 503]) {
      assert.equal(isCacheable("GET", response(status)), false, String(status));
    }
  });

  test("an explicit no-store or private is honoured", () => {
    assert.equal(
      isCacheable("GET", response(200, { "cache-control": "no-store" })),
      false,
    );
    assert.equal(
      isCacheable("GET", response(200, { "cache-control": "private" })),
      false,
    );
  });
});

describe("the stored copy", () => {
  test("carries a bounded lifetime", () => {
    const stored = storedCopy(new Response("body", { status: 200 }));
    assert.equal(stored.headers.get("cache-control"), STORED_CACHE_CONTROL);
  });

  test("does not change what the reader receives", async () => {
    // The origin sends no `Cache-Control` on a docs page — it revalidates with
    // an ETag — and the edge lane must not quietly start pinning pages in
    // browsers, where no purge can reach them. The TTL goes on the copy only.
    const served = new Response("body", {
      status: 200,
      headers: { "content-type": "text/html" },
    });
    storedCopy(served.clone());

    assert.equal(served.headers.get("cache-control"), null);
    assert.equal(await served.text(), "body");
  });

  test("preserves status and headers", () => {
    const stored = storedCopy(
      new Response("body", { status: 404, headers: { "content-type": "text/html" } }),
    );
    assert.equal(stored.status, 404);
    assert.equal(stored.headers.get("content-type"), "text/html");
  });
});

describe("serving a HEAD from a cached GET", () => {
  test("drops the body but keeps the headers", async () => {
    const cached = new Response("the whole page", {
      status: 200,
      headers: { "content-type": "text/html", "content-length": "14" },
    });

    const head = bodilessForHead("HEAD", cached);

    assert.equal(head.body, null);
    assert.equal(head.status, 200);
    assert.equal(head.headers.get("content-length"), "14");
    assert.equal(head.headers.get("content-type"), "text/html");
  });

  test("leaves a GET untouched", async () => {
    const cached = new Response("the whole page", { status: 200 });
    assert.equal(await bodilessForHead("GET", cached).text(), "the whole page");
  });
});

describe("the generated version module", () => {
  test("exists and looks like an artifact hash", async () => {
    // `build-capsule.sh` writes it next to the `.wasm`. If it is missing the
    // Worker will not bundle, so failing here is the earlier, clearer error.
    const path = fileURLToPath(new URL("../build/capsule-version.js", import.meta.url));
    let source;
    try {
      source = await readFile(path, "utf8");
    } catch (error) {
      if (error.code === "ENOENT") {
        assert.fail("run edge/build-capsule.sh first: build/capsule-version.js is missing");
      }
      throw error;
    }

    const match = source.match(/export const CAPSULE_VERSION = "([0-9a-f]+)"/);
    assert.ok(match, `no CAPSULE_VERSION export in:\n${source}`);
    assert.equal(match[1].length, 16);
  });
});

describe("the storage TTL never reaches the reader", () => {
  test("a hit serves exactly what a miss serves", () => {
    // `cache.match()` returns the response as *stored*, TTL header included, so
    // forwarding a hit unchanged told the browser to hold the page — a cached
    // 404 included — for a day no purge could shorten, while a miss said
    // nothing. A hit and a miss must be indistinguishable.
    const served = new Response("page", {
      status: 200,
      headers: { "content-type": "text/html" },
    });
    const fromCache = storedCopy(served.clone());

    const hit = withoutStorageTtl(fromCache);

    assert.deepEqual([...hit.headers].sort(), [...served.headers].sort());
    assert.equal(hit.headers.get("cache-control"), null);
  });

  test("a Cache-Control the handler set is left alone", () => {
    const deliberate = new Response("page", {
      status: 200,
      headers: { "cache-control": "public, max-age=60" },
    });

    assert.equal(
      withoutStorageTtl(deliberate).headers.get("cache-control"),
      "public, max-age=60",
    );
  });

  test("the stored TTL is shared-cache-only", () => {
    // Defence in depth behind the strip above: browsers ignore `s-maxage`, so a
    // future path that forgets to strip it leaves a stale colo rather than a day
    // of un-purgeable copies in readers' browsers.
    assert.match(STORED_CACHE_CONTROL, /^s-maxage=/);
    assert.doesNotMatch(STORED_CACHE_CONTROL, /(^|[^-])max-age=/);
  });
});

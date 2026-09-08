// Colo cache policy for capsule-served responses.
//
// Split out of `index.js` so it can be tested: `index.js` imports the `.wasm`,
// which only a bundler resolves, and everything here is a pure function of its
// arguments.
//
// Two things this module exists to get right, both of which were wrong when the
// Worker cached against the bare request:
//
//   1. A cache entry outlives the deployment that produced it.
//   2. Cloudflare's Cache API is GET-only.

/**
 * The key a capsule response is cached under.
 *
 * **Versioned**, because `caches.default` survives Worker deployments. The
 * guides are embedded in the artifact, so a deploy that changes any guide
 * changes the rendered HTML — and an entry keyed on the URL alone would keep
 * serving the previous deploy's copy from every warmed colo until it expired.
 * A cached 404 is the sharp end of this: publish a new guide and the colos that
 * already answered for that slug would go on saying it does not exist.
 *
 * Keying on the artifact's own hash makes a deploy's entries unreachable the
 * moment the new version ships, with no purge step to remember and nothing to
 * get wrong at deploy time. Old entries are never read again and age out on
 * their own.
 *
 * **Always a `GET`**, because Cloudflare's Cache API only accepts `GET` for
 * both `match` and `put`. Using the inbound request as the key meant a `HEAD`
 * never hit the cache and its `put` rejected inside `waitUntil`. With a
 * synthetic key, a `HEAD` reads the `GET` representation and
 * [`bodilessForHead`] strips the body on the way out.
 *
 * @param {URL} url The inbound request URL.
 * @param {string} version The capsule build's version.
 * @returns {Request}
 */
export function capsuleCacheKey(url, version) {
  const key = new URL(url);
  key.searchParams.set("__capsule", version);
  return new Request(key, { method: "GET" });
}

/**
 * How long a stored entry stays fresh in the colo.
 *
 * Belt to the versioned key's braces. The key alone already makes a deploy's
 * entries unreachable, so this is not what bounds staleness — it is what stops
 * unreachable entries from occupying the cache indefinitely, and what bounds
 * the damage if a future change to the key ever weakens it.
 *
 * Applied only to the *stored* copy, never to what the reader receives: the
 * origin sends no `Cache-Control` on a docs page (it uses `ETag` revalidation),
 * and the edge lane should not quietly start pinning pages in browsers where
 * no purge can reach them.
 */
export const STORED_CACHE_CONTROL = "public, max-age=86400";

/**
 * The copy to store: the response as served, plus a bounded lifetime.
 *
 * @param {Response} response
 * @returns {Response}
 */
export function storedCopy(response) {
  const headers = new Headers(response.headers);
  headers.set("cache-control", STORED_CACHE_CONTROL);
  return new Response(response.body, {
    status: response.status,
    statusText: response.statusText,
    headers,
  });
}

/**
 * Whether a capsule response may go in the colo cache.
 *
 * `GET` only, and that is not merely an API restriction. axum's `MethodRouter`
 * routes `HEAD` to the `GET` handler and strips the body, so the capsule
 * answers a `HEAD` with **zero body bytes and the `GET`'s `content-length`**.
 * Storing that under a key a `GET` would later read serves a truncated page
 * that claims to be the full one — a corrupt response, not just a wasted
 * lookup.
 *
 * Beyond that: successes and the docs 404, which is a stable rendered page
 * rather than an error. A redirect is cheap to re-derive and expensive to get
 * wrong for a day, so it is left uncached.
 *
 * @param {string} method
 * @param {Response} response
 * @returns {boolean}
 */
export function isCacheable(method, response) {
  if (method !== "GET") return false;
  if (response.status !== 200 && response.status !== 404) return false;
  const control = response.headers.get("cache-control") ?? "";
  return !control.includes("no-store") && !control.includes("private");
}

/**
 * Drop the body when the request was a `HEAD`.
 *
 * The cache holds `GET` representations, so a `HEAD` that hits one must have
 * its body removed before the response goes out. Headers — `content-length`
 * included — are preserved, which is exactly what a `HEAD` is for.
 *
 * @param {string} method
 * @param {Response} response
 * @returns {Response}
 */
export function bodilessForHead(method, response) {
  if (method !== "HEAD") return response;
  return new Response(null, {
    status: response.status,
    statusText: response.statusText,
    headers: response.headers,
  });
}

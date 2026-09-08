// Drive one request through an Autumn edge capsule.
//
// This is the half of the shim that is not Cloudflare-specific: give it a
// compiled `WebAssembly.Module` and a request, get back an outcome. `index.js`
// wraps it in a Worker; `test/capsule.test.js` drives the real artifact through
// it under plain Node, which is what makes the protocol testable here at all.
//
// # One instantiation per request
//
// `autumn_edge::serve` loops until stdin reaches EOF, so a host may pipe many
// requests through one process. A Worker cannot: the dialogue is synchronous
// and there is no way to park a returned `_start` between requests. So each
// request gets a fresh instance, fed exactly one request frame and then EOF.
//
// The compiled `WebAssembly.Module` is what actually costs something to
// produce, and that is created once at Worker startup and reused for the
// lifetime of the isolate. Instantiation is linear-memory allocation plus the
// guest's own startup.
//
// # The KV seam
//
// `EdgeCache::get` is a synchronous call inside a running handler, and every
// KV API a CDN offers is asynchronous. The shim closes that gap by resolving
// the whole replica snapshot *before* the guest starts, then answering
// `kv_get` from it inline in `fd_write`. That is exactly the contract `EdgeKv`
// documents — replica-local, opportunistic, staleness expected, a miss always
// legal — so a snapshot is a legitimate implementation of it rather than a
// weakening.
//
// A host that provides no snapshot advertises no capabilities, and a route
// declaring `needs(kv)` then falls through before its handler runs a single
// line. The docs site declares no capabilities at all.

import { ProcExit, Wasi, ALLOWED_IMPORTS } from "./wasi.js";
import { kvValueFrame, parseGuestFrame, requestFrame } from "./wire.js";

/**
 * Serve one request from the capsule.
 *
 * Never throws: every failure — a trap, a malformed frame, a guest that exits
 * without answering — becomes a `capsule_error` fallthrough, because the origin
 * can serve the request and a broken edge response cannot be un-served.
 *
 * @param {WebAssembly.Module} module Compiled capsule.
 * @param {object} request
 * @param {string} request.method
 * @param {string} request.uri Path and query only.
 * @param {Array<[string, string]>} request.headers
 * @param {Uint8Array} [request.body]
 * @param {object} [options]
 * @param {Map<string, Uint8Array>} [options.kv]
 *   Replica snapshot. Its presence is what makes this host advertise the `kv`
 *   capability; omit it and routes needing `kv` fall through.
 * @returns {{op: "response", status: number, headers: Array<[string, string]>, body: Uint8Array}
 *          | {op: "fallthrough", reason: string, detail: string}}
 */
export function serveFromCapsule(module, request, { kv } = {}) {
  const providedCapabilities = kv ? ["kv"] : [];
  /** @type {ReturnType<typeof parseGuestFrame> | null} */
  let outcome = null;
  /** @type {string | null} */
  let frameError = null;

  const wasi = new Wasi({
    onStdoutLine(line) {
      if (line.trim() === "" || outcome !== null) return null;
      let frame;
      try {
        frame = parseGuestFrame(line);
      } catch (error) {
        frameError = `could not parse the guest frame: ${error.message}`;
        return null;
      }
      if (frame.op === "kv_get") {
        return kvValueFrame(kv?.get(frame.key) ?? null);
      }
      outcome = frame;
      return null;
    },
  });

  try {
    const instance = new WebAssembly.Instance(module, wasi.imports);
    wasi.bind(instance);
    wasi.pushStdin(requestFrame(request, providedCapabilities));
    // No second request is coming, so the guest's serve loop should see EOF
    // right after it answers this one.
    wasi.closeStdin();
    instance.exports._start();
  } catch (error) {
    if (!(error instanceof ProcExit) || error.code !== 0) {
      // A trap, a non-zero exit, or a failure to instantiate. If the guest
      // already produced its terminal frame, that answer stands — the guest
      // writes the frame before it returns.
      if (outcome === null) {
        return fallthrough(
          "capsule_error",
          `${error.message}${excerpt(wasi.stderr)}`,
        );
      }
    }
  }

  if (frameError !== null) return fallthrough("capsule_error", frameError);
  if (outcome === null) {
    return fallthrough(
      "capsule_error",
      `the capsule produced no terminal frame${excerpt(wasi.stderr)}`,
    );
  }
  return outcome;
}

/**
 * Check a capsule's imports against what this shim provides.
 *
 * The security claim the edge lane makes is that a capsule has no ambient
 * authority — no filesystem, no sockets, only the dialogue. That claim is a
 * property of the artifact, so it is worth asserting against the artifact
 * rather than trusting it: a dependency that starts opening files would
 * otherwise be discovered in production.
 *
 * @param {WebAssembly.Module} module
 * @returns {string[]} imports this shim does not provide; empty means clean.
 */
export function unexpectedImports(module) {
  return WebAssembly.Module.imports(module)
    .filter(
      ({ module: from, name }) =>
        from === "wasi_snapshot_preview1" && !ALLOWED_IMPORTS.includes(name),
    )
    .map(({ name }) => name)
    .concat(
      WebAssembly.Module.imports(module)
        .filter(({ module: from }) => from !== "wasi_snapshot_preview1")
        .map(({ module: from, name }) => `${from}::${name}`),
    );
}

function fallthrough(reason, detail) {
  return { op: "fallthrough", reason, detail };
}

/** A bounded slice of guest stderr, for a fallthrough detail. */
function excerpt(stderr) {
  const trimmed = stderr.trim();
  if (trimmed === "") return "";
  return `; stderr: ${trimmed.slice(0, 512)}`;
}

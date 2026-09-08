// Autumn edge capsule wire protocol, version 1.
//
// The protocol is NDJSON over the capsule's stdio: one `op`-tagged JSON object
// per line. This module is the whole of it — encoding, decoding, and the two
// header transforms the spec mandates — with no runtime dependencies, so the
// same code runs under Cloudflare Workers and under `node --test`.
//
// See `content/guide/edge.md` § "The wire protocol (version 1)". Every literal
// here is pinned by `test/wire.test.js` against the frame JSON the upstream
// `autumn_edge::wire` tests assert.

/** Protocol version this shim speaks. A mismatch is a `capsule_error`. */
export const WIRE_VERSION = 1;

/** Response header a handler sets to decline a request it cannot serve. */
export const FALLTHROUGH_SENTINEL = "x-autumn-edge-fallthrough";

/**
 * Request headers that never reach a capsule.
 *
 * The guest strips these again on receipt, but a capsule cannot audit the host
 * it runs under, so the strip happens on both sides. The sentinel is in the
 * list because it is an internal control channel: an inbound request must not
 * be able to pre-set it.
 */
export const SENSITIVE_HEADERS = [
  "cookie",
  "authorization",
  "proxy-authorization",
  FALLTHROUGH_SENTINEL,
];

/** Every fallthrough reason version 1 defines. */
export const FALLTHROUGH_REASONS = [
  "unknown_route",
  "method_not_edge_eligible",
  "missing_capability",
  "capsule_error",
];

/**
 * Put headers into canonical wire form: names lowercased, sorted by name,
 * insertion order preserved within a name.
 *
 * `Array.prototype.sort` is stable in every runtime this targets, which is what
 * preserves the relative order of repeated names.
 *
 * @param {Array<[string, string]>} headers
 * @returns {Array<[string, string]>}
 */
export function canonicalizeHeaders(headers) {
  return headers
    .map(([name, value]) => [name.toLowerCase(), value])
    .sort((left, right) => (left[0] < right[0] ? -1 : left[0] > right[0] ? 1 : 0));
}

/**
 * Drop every sensitive header, case-insensitively, preserving the order of
 * everything else.
 *
 * @param {Array<[string, string]>} headers
 * @returns {Array<[string, string]>}
 */
export function stripSensitiveHeaders(headers) {
  return headers.filter(([name]) => !SENSITIVE_HEADERS.includes(name.toLowerCase()));
}

const BASE64_ALPHABET = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

/**
 * Standard base64, with padding — what `base64::engine::general_purpose::STANDARD`
 * produces on the Rust side.
 *
 * Hand-rolled rather than routed through `btoa`: `btoa` takes a binary string,
 * so feeding it a `Uint8Array` means an intermediate `String.fromCharCode`
 * spread that blows the call stack on a body of any size.
 *
 * @param {Uint8Array} bytes
 * @returns {string}
 */
export function encodeBase64(bytes) {
  let out = "";
  let i = 0;
  for (; i + 2 < bytes.length; i += 3) {
    const triple = (bytes[i] << 16) | (bytes[i + 1] << 8) | bytes[i + 2];
    out += BASE64_ALPHABET[(triple >> 18) & 63];
    out += BASE64_ALPHABET[(triple >> 12) & 63];
    out += BASE64_ALPHABET[(triple >> 6) & 63];
    out += BASE64_ALPHABET[triple & 63];
  }
  const remaining = bytes.length - i;
  if (remaining === 1) {
    const chunk = bytes[i] << 16;
    out += BASE64_ALPHABET[(chunk >> 18) & 63];
    out += BASE64_ALPHABET[(chunk >> 12) & 63];
    out += "==";
  } else if (remaining === 2) {
    const chunk = (bytes[i] << 16) | (bytes[i + 1] << 8);
    out += BASE64_ALPHABET[(chunk >> 18) & 63];
    out += BASE64_ALPHABET[(chunk >> 12) & 63];
    out += BASE64_ALPHABET[(chunk >> 6) & 63];
    out += "=";
  }
  return out;
}

/**
 * Decode standard base64 into bytes. Throws on anything that is not base64 —
 * the caller turns that into a `capsule_error`, which is a fallthrough.
 *
 * @param {string} encoded
 * @returns {Uint8Array}
 */
export function decodeBase64(encoded) {
  if (encoded === "") return new Uint8Array(0);
  const binary = atob(encoded);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) bytes[i] = binary.charCodeAt(i);
  return bytes;
}

/**
 * Build the `request` frame for one inbound request.
 *
 * `headers` is stripped and canonicalized here, exactly as
 * `EdgeRequest::into_host_frame` does, so a capsule sees the same bytes no
 * matter which host framed them.
 *
 * @param {object} request
 * @param {string} request.method
 * @param {string} request.uri            Path and query only — never the origin.
 * @param {Array<[string, string]>} request.headers
 * @param {Uint8Array} [request.body]
 * @param {string[]} providedCapabilities Seams this host can mediate.
 * @returns {string} One NDJSON line, newline included.
 */
export function requestFrame({ method, uri, headers, body }, providedCapabilities) {
  return `${JSON.stringify({
    op: "request",
    wire_version: WIRE_VERSION,
    provided_capabilities: providedCapabilities,
    method,
    uri,
    headers: canonicalizeHeaders(stripSensitiveHeaders(headers)),
    body_b64: encodeBase64(body ?? new Uint8Array(0)),
  })}\n`;
}

/**
 * Build the `kv_value` frame answering a `kv_get`. `null` is a miss, which is
 * always a legal answer.
 *
 * @param {Uint8Array | null} value
 * @returns {string}
 */
export function kvValueFrame(value) {
  return `${JSON.stringify({
    op: "kv_value",
    value_b64: value === null ? null : encodeBase64(value),
  })}\n`;
}

/**
 * Parse one guest frame.
 *
 * Returns a discriminated object: `{op: "kv_get", key}`, `{op: "response",
 * status, headers, body}`, or `{op: "fallthrough", reason, detail}`. Throws on
 * anything malformed, including a `reason` this version does not know — the
 * caller degrades that to a fallthrough rather than guessing.
 *
 * @param {string} line
 */
export function parseGuestFrame(line) {
  const frame = JSON.parse(line);
  switch (frame.op) {
    case "kv_get":
      if (typeof frame.key !== "string") throw new TypeError("kv_get without a string key");
      return { op: "kv_get", key: frame.key };
    case "response":
      return {
        op: "response",
        status: frame.status,
        headers: frame.headers ?? [],
        body: decodeBase64(frame.body_b64 ?? ""),
      };
    case "fallthrough":
      if (!FALLTHROUGH_REASONS.includes(frame.reason)) {
        throw new TypeError(`unknown edge fallthrough reason \`${frame.reason}\``);
      }
      return { op: "fallthrough", reason: frame.reason, detail: frame.detail ?? "" };
    default:
      throw new TypeError(`unknown guest frame op \`${frame.op}\``);
  }
}

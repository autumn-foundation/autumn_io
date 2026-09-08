// The wire protocol, checked against the frames upstream documents.
//
// `autumn_edge::wire` asserts these exact strings in its own tests, so any
// drift between the Rust encoder and this one shows up here rather than as a
// capsule that silently falls through every request in production.

import assert from "node:assert/strict";
import test, { describe } from "node:test";

import {
  canonicalizeHeaders,
  decodeBase64,
  encodeBase64,
  kvValueFrame,
  parseGuestFrame,
  requestFrame,
  stripSensitiveHeaders,
} from "../src/wire.js";

describe("request frames", () => {
  test("match the documented JSON byte for byte", () => {
    assert.equal(
      requestFrame(
        { method: "GET", uri: "/x?y=z", headers: [["Accept", "text/html"]] },
        ["kv"],
      ),
      '{"op":"request","wire_version":1,"provided_capabilities":["kv"],' +
        '"method":"GET","uri":"/x?y=z","headers":[["accept","text/html"]],"body_b64":""}\n',
    );
  });

  test("strip credentials before they reach the capsule", () => {
    const frame = JSON.parse(
      requestFrame(
        {
          method: "GET",
          uri: "/",
          headers: [
            ["Cookie", "sid=1"],
            ["accept", "text/html"],
            ["AUTHORIZATION", "Bearer x"],
            ["Proxy-Authorization", "Basic y"],
            ["X-Autumn-Edge-Fallthrough", "unknown_route"],
          ],
        },
        [],
      ),
    );
    assert.deepEqual(frame.headers, [["accept", "text/html"]]);
  });
});

describe("header canonicalization", () => {
  test("lowercases, sorts, and keeps multi-value order", () => {
    assert.deepEqual(
      canonicalizeHeaders([
        ["X-Foo", "1"],
        ["accept", "a"],
        ["X-FOO", "2"],
        ["Accept", "b"],
      ]),
      [
        ["accept", "a"],
        ["accept", "b"],
        ["x-foo", "1"],
        ["x-foo", "2"],
      ],
    );
  });

  test("is idempotent", () => {
    const once = canonicalizeHeaders([
      ["B", "2"],
      ["a", "1"],
    ]);
    assert.deepEqual(canonicalizeHeaders(once), once);
  });

  test("credential stripping is case-insensitive", () => {
    assert.deepEqual(
      stripSensitiveHeaders([
        ["Cookie", "sid=1"],
        ["accept", "text/html"],
        ["x-keep", "yes"],
      ]),
      [
        ["accept", "text/html"],
        ["x-keep", "yes"],
      ],
    );
  });
});

describe("base64", () => {
  test("round trips binary", () => {
    const bytes = new Uint8Array([0, 255, 10, 13, 0x7f, 97]);
    assert.deepEqual(decodeBase64(encodeBase64(bytes)), bytes);
  });

  test("encodes the empty body as the empty string", () => {
    assert.equal(encodeBase64(new Uint8Array(0)), "");
    assert.deepEqual(decodeBase64(""), new Uint8Array(0));
  });

  test("pads the way the Rust STANDARD engine does", () => {
    const encode = (s) => encodeBase64(new TextEncoder().encode(s));
    assert.equal(encode("hi"), "aGk=");
    assert.equal(encode("h"), "aA==");
    assert.equal(encode("hey"), "aGV5");
  });

  test("survives a body larger than a call-stack spread", () => {
    // `String.fromCharCode(...bytes)` blows up around 100k arguments; this is
    // why the encoder is hand-rolled rather than routed through `btoa`.
    const big = new Uint8Array(400_000).fill(65);
    assert.deepEqual(decodeBase64(encodeBase64(big)), big);
  });
});

describe("kv frames", () => {
  test("a hit carries base64 bytes", () => {
    assert.equal(
      kvValueFrame(new TextEncoder().encode("hi")),
      '{"op":"kv_value","value_b64":"aGk="}\n',
    );
  });

  test("a miss is null, not an empty string", () => {
    assert.equal(kvValueFrame(null), '{"op":"kv_value","value_b64":null}\n');
  });
});

describe("guest frames", () => {
  test("a response decodes its body", () => {
    const frame = parseGuestFrame(
      '{"op":"response","status":200,"headers":[["content-type","text/plain"]],"body_b64":"aGk="}',
    );
    assert.equal(frame.op, "response");
    assert.equal(frame.status, 200);
    assert.equal(new TextDecoder().decode(frame.body), "hi");
  });

  test("a fallthrough carries a known reason", () => {
    const frame = parseGuestFrame(
      '{"op":"fallthrough","reason":"unknown_route","detail":"no edge route matches /nope"}',
    );
    assert.equal(frame.reason, "unknown_route");
  });

  test("an unknown reason is refused rather than guessed at", () => {
    assert.throws(
      () => parseGuestFrame('{"op":"fallthrough","reason":"vibes","detail":""}'),
      /unknown edge fallthrough reason/,
    );
  });

  test("an unknown op is refused", () => {
    assert.throws(() => parseGuestFrame('{"op":"kv_put","key":"k"}'), /unknown guest frame op/);
  });
});

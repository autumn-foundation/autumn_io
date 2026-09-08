// The deployment version, which is the colo cache key's version.
//
// Its whole job is to make a deployment's cache entries unreachable whenever a
// served byte could differ. The first version hashed only the capsule, and that
// left a real hole: a deploy changing `security-headers.json` — say, to restore
// a fifth security header — produces a byte-identical `.wasm`, so every warmed
// colo would keep serving the previous headers with the new Worker logic never
// running.
//
// So the property under test is not "the hash is a hash". It is: change
// anything that decides a response byte, and the version moves; change nothing,
// and it does not.

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { cp, mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test, { after, before, describe } from "node:test";

const EDGE_DIR = fileURLToPath(new URL("../..", import.meta.url));
const SCRIPT = join(EDGE_DIR, "compute-version.sh");

/** A throwaway copy of `edge/` plus a stand-in artifact, safe to mutate. */
async function fixture() {
  const root = await mkdtemp(join(tmpdir(), "autumn-edge-version-"));
  const edge = join(root, "edge");
  await cp(EDGE_DIR, edge, {
    recursive: true,
    filter: (source) => !source.includes(`${join("worker", "build")}`),
  });
  const wasm = join(root, "edge-capsule.wasm");
  await writeFile(wasm, "pretend this is a capsule");
  return { root, edge, wasm };
}

function version(edge, wasm) {
  return execFileSync(SCRIPT, [edge, wasm], { encoding: "utf8" }).trim();
}

describe("the deployment version", () => {
  let fx;
  before(async () => {
    fx = await fixture();
  });
  after(async () => {
    await rm(fx.root, { recursive: true, force: true });
  });

  test("is sixteen hex digits", () => {
    assert.match(version(fx.edge, fx.wasm), /^[0-9a-f]{16}$/);
  });

  test("is stable when nothing changes", () => {
    // Load bearing, not a tautology: a version that moved on every build would
    // throw away every warm cache for nothing. This is also why the script
    // hashes file *contents* and never `sha256sum`'s path-bearing output —
    // an absolute path differs between a laptop and CI.
    assert.equal(version(fx.edge, fx.wasm), version(fx.edge, fx.wasm));
  });

  test("moves when the capsule changes", async () => {
    const before = version(fx.edge, fx.wasm);
    await writeFile(fx.wasm, "a different capsule");
    assert.notEqual(version(fx.edge, fx.wasm), before);
  });

  test("moves when the security-header list changes", async () => {
    // The finding that prompted this test. A capsule-only hash misses it
    // entirely, and it is the change whose staleness matters most.
    const path = join(fx.edge, "security-headers.json");
    const before = version(fx.edge, fx.wasm);
    const config = JSON.parse(await readFile(path, "utf8"));
    config.headers["x-permitted-cross-domain-policies"] = "none";
    await writeFile(path, JSON.stringify(config, null, 2));

    assert.notEqual(version(fx.edge, fx.wasm), before);
  });

  test("moves when Worker logic changes", async () => {
    const path = join(fx.edge, "worker", "src", "index.js");
    const before = version(fx.edge, fx.wasm);
    await writeFile(path, `${await readFile(path, "utf8")}\n// a behavioural edit\n`);

    assert.notEqual(version(fx.edge, fx.wasm), before);
  });

  test("moves when the origin or routes change", async () => {
    const path = join(fx.edge, "worker", "wrangler.toml");
    const before = version(fx.edge, fx.wasm);
    await writeFile(
      path,
      (await readFile(path, "utf8")).replace(
        "https://autumn-io.fly.dev",
        "https://somewhere-else.example",
      ),
    );

    assert.notEqual(version(fx.edge, fx.wasm), before);
  });
});

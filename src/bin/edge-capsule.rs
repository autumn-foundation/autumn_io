//! The edge capsule: the docs site's read path, compiled for `wasm32-wasip1`.
//!
//! `autumn build` produces this alongside the native binary, as
//! `target/wasm32-wasip1/release/edge-capsule.wasm`. A CDN shim loads it,
//! speaks the NDJSON wire protocol on its stdio, and forwards anything it
//! declines to the origin. The shim this repository deploys is
//! `edge/worker/`, a Cloudflare Worker; the protocol it implements is
//! documented in `content/guide/edge.md` § "The wire protocol (version 1)".
//!
//! There is nothing else to a capsule's `main`. `serve` reads request frames
//! until stdin reaches EOF and answers each with exactly one terminal frame,
//! so a host may run one request per instantiation or many down one pipe.

fn main() {
    autumn_edge::serve(autumn_io::edge::edge_routes());
}

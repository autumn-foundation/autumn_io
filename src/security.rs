//! `Content-Security-Policy` nonce for Cloudflare's JavaScript Detections.
//!
//! See <https://developers.cloudflare.com/bots/additional-configurations/javascript-detections/>:
//! Cloudflare's bot-management JS injects an inline `<script>` into every
//! response to run its browser fingerprinting, and it parses the response's
//! `Content-Security-Policy` header for a `script-src` nonce it can copy onto
//! that script. Without one, the framework default CSP's `script-src 'self'`
//! (no `'unsafe-inline'`, no nonce) blocks it outright.
//!
//! `autumn.toml` empties `[security.headers].content_security_policy` (the
//! framework's own "escape hatch" for not emitting a header at all) and turns
//! on `[security.headers.csp_nonce]`. That combination makes
//! `SecurityHeadersLayer` mint a fresh nonce per request and thread it through
//! the `CspNonce` extractor, while emitting no `Content-Security-Policy` header
//! of its own — [`layer`] extracts that nonce and emits the header itself.
//!
//! The framework's own `csp_nonce` toggle does this natively, but *only* by
//! swapping in a fixed template that also nonces `style-src` and drops its
//! `'unsafe-inline'`. This site can't take that template: its syntax-highlighted
//! code blocks (`docs::highlight_code`) put `style="color:..."` on every token
//! `<span>`, and a CSP nonce does not cover the `style` *attribute* — per the
//! CSP3 algorithm for matching an element to a source list, nonce-source only
//! matches `<script>`/`<style>` *elements*; an inline `style` attribute needs
//! `'unsafe-inline'` (or a hash paired with `'unsafe-hashes'`). Nonce-ing
//! `style-src` here would silently strip all syntax-highlighting color from
//! every guide page, so this crafts the header by hand instead: the framework's
//! own default policy, with a nonce spliced into `script-src` only.
//!
//! KNOWN LIMITATION: `/mcp` gets no `Content-Security-Policy` at all, not even
//! the static default it used to carry. The framework applies its own
//! `SecurityHeadersLayer` directly to the `/mcp` router in addition to the
//! outer one this layer sits inside (see `autumn-web`'s `router.rs`), and that
//! direct application is unconditionally outer to anything registered via
//! `AppBuilder::layer` — so [`apply_csp_nonce`] never runs for it, confirmed by
//! hitting `/mcp` directly. Emptying `content_security_policy` is what stops
//! that *outer* wrap from clobbering the header this layer sets on ordinary
//! pages (the framework pushes its configured CSP unconditionally whenever the
//! string is non-empty, nonce or not) — the same setting that fixes the pages
//! Cloudflare's script injection actually targets costs `/mcp` its header.
//! Accepted rather than chased further: `/mcp` is a JSON API with no HTML
//! document for a browser to apply `script-src`/`style-src` against, its other
//! security headers (`X-Frame-Options`, etc.) are unaffected, and it isn't a
//! surface Cloudflare's JavaScript Detections injects into.

use autumn_web::reexports::axum::extract::Request;
use autumn_web::reexports::axum::middleware::{self, Next};
use autumn_web::reexports::axum::response::Response;
use autumn_web::reexports::http::{HeaderValue, header};
use autumn_web::security::{CspNonce, default_content_security_policy};

/// Wraps [`apply_csp_nonce`] as a layer the app can register with
/// `AppBuilder::layer`/`TestApp::layer`.
pub fn layer() -> impl autumn_web::app::IntoAppLayer {
    middleware::from_fn(apply_csp_nonce)
}

/// Sets `Content-Security-Policy` with the per-request nonce spliced into
/// `script-src`. A missing nonce (`csp_nonce` disabled) leaves any
/// `Content-Security-Policy` the response already carries untouched, rather
/// than emitting a policy with no nonce for Cloudflare to find.
async fn apply_csp_nonce(nonce: Option<CspNonce>, request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;

    if let Some(nonce) = nonce {
        let csp = default_content_security_policy().replacen(
            "script-src 'self'",
            &format!("script-src 'self' 'nonce-{}'", nonce.value()),
            1,
        );
        if let Ok(value) = HeaderValue::from_str(&csp) {
            response
                .headers_mut()
                .insert(header::CONTENT_SECURITY_POLICY, value);
        }
    }

    response
}

#[cfg(test)]
mod tests {
    use autumn_web::config::MockEnv;
    use autumn_web::test::TestApp;

    fn loaded_config() -> autumn_web::config::AutumnConfig {
        let env = MockEnv::new().with("AUTUMN_MANIFEST_DIR", env!("CARGO_MANIFEST_DIR"));
        autumn_web::config::AutumnConfig::load_with_env(&env).expect("config should load")
    }

    fn client(config: autumn_web::config::AutumnConfig) -> autumn_web::test::TestClient {
        TestApp::new()
            .routes(crate::app_routes())
            .config(config)
            .layer(super::layer())
            .build()
    }

    #[tokio::test]
    async fn splices_a_nonce_into_script_src_only() {
        let response = client(loaded_config()).get("/").send().await;
        let csp = response
            .header("content-security-policy")
            .expect("CSP header should be present")
            .to_owned();

        assert!(
            csp.contains("script-src 'self' 'nonce-"),
            "script-src must carry a nonce for Cloudflare to copy: {csp}"
        );
        assert!(
            csp.contains("style-src 'self' 'unsafe-inline'"),
            "style-src must keep unsafe-inline (syntax-highlighted code blocks \
             use inline style attributes, which a nonce cannot cover): {csp}"
        );
    }

    #[tokio::test]
    async fn nonce_differs_between_requests() {
        let csp1 = client(loaded_config())
            .get("/")
            .send()
            .await
            .header("content-security-policy")
            .expect("CSP header should be present")
            .to_owned();
        let csp2 = client(loaded_config())
            .get("/")
            .send()
            .await
            .header("content-security-policy")
            .expect("CSP header should be present")
            .to_owned();

        assert_ne!(csp1, csp2, "each request must mint its own nonce");
    }

    #[tokio::test]
    async fn no_csp_header_when_csp_nonce_is_disabled() {
        let mut config = loaded_config();
        config.security.headers.csp_nonce.enabled = false;

        let response = client(config).get("/").send().await;

        assert!(
            response.header("content-security-policy").is_none(),
            "with no nonce to splice in, this layer must not fabricate a \
             nonce-less CSP that Cloudflare can't use anyway"
        );
    }
}

//! Auth.md discovery: `/auth.md`, protected-resource and authorization-server
//! metadata, as published for agent registration.

use autumn_web::test::{TestApp, TestClient};
use serde_json::Value;

fn app() -> TestClient {
    TestApp::new().routes(autumn_io::app_routes()).build()
}

async fn json(path: &str) -> Value {
    let response = app().get(path).send().await;
    response.assert_status(200);
    serde_json::from_str(&response.text()).expect("valid JSON")
}

#[tokio::test]
async fn auth_md_is_markdown_with_an_auth_md_heading() {
    let response = app().get("/auth.md").send().await;
    response.assert_status(200);
    assert_eq!(
        response.header("content-type"),
        Some("text/markdown; charset=utf-8")
    );
    let body = response.text();
    assert!(body.starts_with("# auth.md"));
    assert!(body.contains("anonymous"));
}

#[tokio::test]
async fn protected_resource_metadata_names_the_authorization_server() {
    let prm = json("/.well-known/oauth-protected-resource").await;
    assert_eq!(prm["resource"], "https://autumn-web.app");
    assert_eq!(prm["authorization_servers"][0], "https://autumn-web.app");
    assert!(
        prm["scopes_supported"]
            .as_array()
            .is_some_and(|s| !s.is_empty())
    );
    assert!(
        prm["bearer_methods_supported"]
            .as_array()
            .is_some_and(|m| m.iter().any(|v| v == "header"))
    );
}

#[tokio::test]
async fn authorization_server_metadata_has_agent_auth_and_matching_issuer() {
    let prm = json("/.well-known/oauth-protected-resource").await;
    let meta = json("/.well-known/oauth-authorization-server").await;
    assert_eq!(meta["issuer"], prm["authorization_servers"][0]);

    let agent = &meta["agent_auth"];
    assert!(meta.get("response_types_supported").is_none());
    assert!(meta.get("grant_types_supported").is_none());
    assert_eq!(agent["register_uri"], "https://autumn-web.app/auth.md");
    assert_eq!(agent["skill"], "https://autumn-web.app/auth.md");
    assert_eq!(agent["identity_types_supported"][0], "anonymous");
    assert_eq!(agent["anonymous"]["credential_types_supported"][0], "none");
}

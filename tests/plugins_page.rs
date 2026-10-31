//! The `/plugins` catalog: the page, its Markdown twin, the links to it, and
//! the shape of the catalog data.

use std::collections::HashSet;

use autumn_io::plugins::{PLUGINS, PLUGINS_PATH, PluginCategory, plugins_in};
use autumn_web::test::{TestApp, TestClient};

fn app() -> TestClient {
    TestApp::new()
        .routes(autumn_io::app_routes())
        .layer(autumn_io::response_compression_layer())
        .build()
}

#[tokio::test]
async fn the_page_lists_every_plugin_with_its_three_links() {
    let body = app()
        .get(PLUGINS_PATH)
        .send()
        .await
        .assert_status(200)
        .text();

    assert!(body.contains(r#"<h1 id="page-title">Plugins</h1>"#));
    assert!(body.contains(r#"<link rel="canonical" href="https://autumn-web.app/plugins">"#));
    for plugin in PLUGINS {
        for needle in [
            format!("<code>{}</code>", plugin.crate_name),
            format!(r#"href="https://crates.io/crates/{}""#, plugin.crate_name),
            format!(r#"href="https://docs.rs/{}""#, plugin.crate_name),
            format!(r#"href="{}""#, plugin.repository),
        ] {
            assert!(body.contains(&needle), "{needle} is missing from /plugins");
        }
    }
    for category in PluginCategory::ALL {
        assert!(
            body.contains(&format!(r##"href="#{}""##, category.id())),
            "the jump links skip {}",
            category.label(),
        );
        assert!(body.contains(&format!(r#"id="{}""#, category.id())));
    }
}

#[tokio::test]
async fn the_header_marks_the_page_as_current() {
    let body = app()
        .get(PLUGINS_PATH)
        .send()
        .await
        .assert_status(200)
        .text();
    assert!(body.contains(r#"<a class="active" aria-current="page" href="/plugins">Plugins</a>"#));
}

#[tokio::test]
async fn the_home_page_and_the_guides_link_to_the_catalog() {
    let app = app();
    for path in ["/", "/docs/getting-started"] {
        let body = app.get(path).send().await.assert_status(200).text();
        assert!(
            body.contains(r#"<a href="/plugins">Plugins</a>"#),
            "{path} has no header link to /plugins",
        );
    }
    let home = app.get("/").send().await.text();
    assert!(home.contains(&format!("{} published plugins", PLUGINS.len())));
    assert!(home.contains(r#"href="/plugins">Browse plugins</a>"#));
}

#[tokio::test]
async fn the_markdown_twin_lists_every_plugin() {
    let body = app()
        .get(PLUGINS_PATH)
        .header("accept", "text/markdown")
        .send()
        .await
        .assert_status(200)
        .text();

    assert!(body.starts_with("# Plugins\n"));
    for category in PluginCategory::ALL {
        assert!(body.contains(&format!("## {}\n", category.label())));
    }
    for plugin in PLUGINS {
        assert!(body.contains(&format!(
            "- **{}**: {} [crates.io](https://crates.io/crates/{})",
            plugin.crate_name, plugin.summary, plugin.crate_name,
        )));
    }

    let home = app()
        .get("/")
        .header("accept", "text/markdown")
        .send()
        .await
        .text();
    assert!(home.contains("(https://autumn-web.app/plugins)"));
}

#[tokio::test]
async fn the_sitemap_lists_the_catalog() {
    let body = app()
        .get("/sitemap.xml")
        .send()
        .await
        .assert_status(200)
        .text();
    assert!(body.contains("<loc>https://autumn-web.app/plugins</loc>"));
}

#[test]
fn every_plugin_is_listed_once_and_every_category_has_one() {
    let mut names = HashSet::new();
    for plugin in PLUGINS {
        assert!(
            names.insert(plugin.crate_name),
            "{} is listed twice",
            plugin.crate_name,
        );
    }
    for category in PluginCategory::ALL {
        assert!(
            plugins_in(category).next().is_some(),
            "{} has no plugins",
            category.label(),
        );
    }
}

#[test]
fn each_category_is_sorted_by_crate_name() {
    for category in PluginCategory::ALL {
        let names: Vec<_> = plugins_in(category).map(|p| p.crate_name).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted, "{} is out of order", category.label());
    }
}

#[test]
fn each_entry_has_a_crate_name_a_github_source_and_one_sentence() {
    let category_ids: HashSet<_> = PluginCategory::ALL.iter().map(|c| c.id()).collect();
    assert_eq!(category_ids.len(), PluginCategory::ALL.len());

    for plugin in PLUGINS {
        let name = plugin.crate_name;
        assert!(name.starts_with("autumn-"), "{name} is not an Autumn crate");
        assert!(
            name.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-'),
            "{name} is not a valid crate name",
        );
        assert!(
            plugin.repository.starts_with("https://github.com/")
                && !plugin.repository.ends_with('/'),
            "{name} has a bad source URL",
        );
        assert!(
            plugin.summary.ends_with('.') && !plugin.summary.contains('\n'),
            "{name} needs a one-sentence summary",
        );
    }
}

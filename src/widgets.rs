//! The docs-search widget markup, rendered without `autumn-web`.
//!
//! `src/site.rs` renders the docs layout for both lanes — the origin binary and
//! the `wasm32-wasip1` capsule — and the capsule has no `autumn-web` in its
//! dependency graph, so the two `autumn_web::widgets` calls the layout used to
//! make had to come across.
//!
//! This is a port, and the point of a port is that it produces the *same bytes*.
//! The edge lane's guarantee is that a page served from the CDN is
//! byte-identical to the same page served from the origin (see
//! `content/guide/edge.md` § "Byte-identity"), and one lane rendering a
//! framework widget while the other renders a lookalike would break that
//! guarantee on every docs page at once. So `tests/widget_parity.rs` asserts
//! these functions against `autumn_web::widgets` on the native target, where
//! both are in the graph.
//!
//! Only the two entry points the docs layout actually calls are ported. The
//! configuration surface is narrowed to what this site passes: `GET`, a `#id`
//! target, a placeholder, a minimum length, and an indicator selector.

use maud::{Markup, html};

/// Where the framework serves htmx from.
///
/// Mirrors `autumn_web::htmx::HTMX_JS_PATH`; pinned by `tests/widget_parity.rs`.
pub const HTMX_JS_PATH: &str = "/static/js/htmx.min.js";

/// The debounce the framework's `ActiveSearchConfig` defaults to, in
/// milliseconds. The docs search box does not override it.
const DEFAULT_DEBOUNCE_MS: u32 = 300;

/// The query parameter name the framework's config defaults to.
const DEFAULT_PARAM_NAME: &str = "q";

/// The subset of `autumn_web::widgets::ActiveSearchConfig` this site uses.
#[derive(Clone, Copy, Debug)]
pub struct ActiveSearchConfig<'a> {
    /// URL of the server-side search handler.
    pub action: &'a str,
    /// CSS `#id` selector for the element that receives rendered results.
    pub target: &'a str,
    /// CSS selector for an element shown while the request is in flight.
    pub indicator: Option<&'a str>,
    /// Minimum character count before a search fires.
    pub min_length: u32,
    /// Placeholder text for the search input.
    pub placeholder: Option<&'a str>,
}

impl<'a> ActiveSearchConfig<'a> {
    /// A configuration with the framework's defaults for everything this site
    /// does not set.
    #[must_use]
    pub const fn new(action: &'a str, target: &'a str) -> Self {
        Self {
            action,
            target,
            indicator: None,
            min_length: 1,
            placeholder: None,
        }
    }

    /// Set the placeholder text.
    #[must_use]
    pub const fn placeholder(mut self, placeholder: &'a str) -> Self {
        self.placeholder = Some(placeholder);
        self
    }

    /// Set the minimum query length before a search is triggered.
    #[must_use]
    pub const fn min_length(mut self, length: u32) -> Self {
        self.min_length = length;
        self
    }

    /// Set the selector of the element htmx marks while a request is in flight.
    #[must_use]
    pub const fn indicator(mut self, selector: &'a str) -> Self {
        self.indicator = Some(selector);
        self
    }
}

/// Strip a leading `#` from a CSS id selector to get a bare element id.
///
/// `aria-controls` takes an id (no `#`), while `hx-target` takes a selector.
fn selector_to_id(selector: &str) -> &str {
    selector.strip_prefix('#').unwrap_or(selector)
}

/// Render the labelled `<input type="search">` with its htmx attributes.
fn active_search_input(id: &str, label: &str, config: &ActiveSearchConfig<'_>) -> Markup {
    let trigger = format!("input changed delay:{DEFAULT_DEBOUNCE_MS}ms");
    let aria_controls = selector_to_id(config.target);

    html! {
        div class="autumn-search" {
            label for=(id) class="autumn-search__label" { (label) }
            input
                type="search"
                id=(id)
                name=(DEFAULT_PARAM_NAME)
                autocomplete="off"
                aria-controls=(aria_controls)
                placeholder=[config.placeholder]
                class="autumn-search__input"
                data-ac-min-length=(config.min_length)
                hx-get=(config.action)
                hx-trigger=(trigger)
                hx-target=(config.target)
                hx-indicator=[config.indicator];
        }
    }
}

/// Render the results container the input targets.
///
/// `role="status"` plus `aria-live="polite"` is what makes a screen reader
/// announce result updates without moving keyboard focus.
fn active_search_results(id: &str) -> Markup {
    html! {
        div
            id=(id)
            role="status"
            aria-live="polite"
            aria-atomic="true" {}
    }
}

/// Render the complete active-search widget: input, results container, and a
/// `<noscript>` form that works with JavaScript off.
///
/// `config.target` must be a `#id` selector; the results container's `id` is
/// derived from it, so the input's `hx-target` and the container can never
/// disagree.
#[must_use]
pub fn active_search(id: &str, label: &str, config: &ActiveSearchConfig<'_>) -> Markup {
    debug_assert!(
        config.target.starts_with('#'),
        "active_search: config.target must be a #id selector, got {:?}",
        config.target
    );
    let results_id = selector_to_id(config.target).to_owned();

    html! {
        div id=(format!("{id}-wrapper")) {
            (active_search_input(id, label, config))
            (active_search_results(&results_id))
            noscript {
                form action=(config.action) method="get" {
                    label for=(format!("{id}-noscript")) { (label) }
                    input
                        type="search"
                        id=(format!("{id}-noscript"))
                        name=(DEFAULT_PARAM_NAME)
                        placeholder=[config.placeholder];
                    button type="submit" { "Search" }
                }
            }
        }
    }
}

/// Render the empty-state partial a search handler returns when nothing
/// matches.
#[must_use]
pub fn active_search_empty_state(message: &str) -> Markup {
    html! {
        div
            role="status"
            aria-live="polite"
            class="search-empty" {
            (message)
        }
    }
}

//! Markdown content negotiation: one URL, two representations.
//!
//! An agent reading this site does not want the page — it wants the words on
//! it. Served HTML, it has to strip a document head, a skip link, a header, a
//! sidebar of 140 links, syntax-highlighted `<span>`s around every token of
//! every code block, and a footer, to recover Markdown the site already has in
//! memory. This module lets it ask for that Markdown directly, at the same URL
//! a browser uses, with the header the convention specifies:
//!
//! ```text
//! curl -H 'Accept: text/markdown' https://autumn-web.app/docs/getting-started
//! ```
//!
//! HTML stays the default. Only a request that *names* `text/markdown` — and
//! ranks it above `text/html` — gets Markdown, so a browser (which sends
//! `text/html,…,*/*;q=0.8`) and a bare `curl` (`*/*`) are unaffected.
//!
//! # Why not `autumn_web::negotiate::Negotiate`
//!
//! The framework's extractor negotiates HTML against **JSON** and hard-codes
//! that pair: its `Accept` parser records slots for `text/html`,
//! `application/json`, `application/problem+json`, `text/*`, `application/*`
//! and `*/*`, and both the parser and the `AcceptQualities` it returns are
//! `pub(crate)`. There is no seam to add a third media type through, so the
//! parse below is this site's own — but the *policy* on top of it is
//! deliberately the framework's, so one site does not answer `Accept` two
//! different ways depending on which route you hit. It departs in exactly one
//! place, for a reason the framework does not have to weigh: see *Only a named
//! `text/markdown` elects Markdown* below.
//!
//! # Resolution policy (RFC 7231 §5.3)
//!
//! Each candidate's *effective* quality comes from the most specific media
//! range that names it — `text/html` or `text/markdown`, then `text/*`, then
//! `*/*`. Then:
//!
//! * an effective `q=0` **forbids** that representation, even if a broader
//!   range would allow it;
//! * among the rest, the higher positive q wins, ties broken by the earlier
//!   entry in the header;
//! * if both are forbidden — or HTML is forbidden and Markdown was never named
//!   — the response is `406 Not Acceptable`.
//!
//! Note that `text/*` is the subtype wildcard for *both* candidates, so
//! `Accept: text/*` expresses no preference between them and gets HTML.
//!
//! ## Only a named `text/markdown` elects Markdown
//!
//! One deliberate departure from the framework's policy: a wildcard can forbid
//! Markdown but never *choose* it. `Accept: text/html;q=0.1, */*;q=1` lifts
//! Markdown above HTML on effective q under a strict reading, and is served
//! HTML here; `Accept: text/html;q=0` — HTML refused, Markdown never mentioned
//! — is a `406` rather than Markdown by elimination.
//!
//! The reason is the edge, not the RFC. Cloudflare cannot express "effective q
//! of `text/markdown` exceeds that of `text/html`" in a cache rule; it can only
//! match the header as a string. Restricting election to the literal media type
//! makes the origin's condition for serving Markdown exactly the condition the
//! bypass rule matches — every request that would be answered in Markdown
//! reaches the origin, instead of some shapes being answered from an HTML cache
//! entry that ignores `Accept`. Nothing an agent or a browser actually sends is
//! affected: `text/markdown` and `text/markdown, */*;q=0.1` still resolve to
//! Markdown, and every browser header still resolves to HTML.
//! `docs/adr/0002-markdown-for-agents.md` records the trade.
//!
//! # Caching
//!
//! Every negotiated response carries `Vary: Accept`, so a cache that honours it
//! keys the two representations apart. Cloudflare, which fronts this site, does
//! not honour a custom `Vary` on HTML — see [`crate::UNCACHEABLE`] and
//! `docs/adr/0002-markdown-for-agents.md` — which is why the Markdown
//! representation is additionally marked `no-store` by
//! [`crate::apply_cache_control`]: an entry stored under a key that ignores
//! `Accept` would serve Markdown to a browser.

use std::convert::Infallible;

use autumn_web::reexports::axum::extract::FromRequestParts;
use autumn_web::reexports::axum::response::{IntoResponse, Response};
use autumn_web::reexports::http::request::Parts;
use autumn_web::reexports::http::{HeaderMap, HeaderValue, StatusCode, header};

/// Media type of the Markdown representation, as sent in `Content-Type`.
pub const MARKDOWN_CONTENT_TYPE: &str = "text/markdown; charset=utf-8";

/// Media type a request names to ask for Markdown.
pub const MARKDOWN_MEDIA_TYPE: &str = "text/markdown";

/// Response header carrying the estimated token count of a Markdown body.
///
/// The convention an agent-facing site is expected to follow (and what
/// Cloudflare's own edge conversion emits), so a caller can decide whether a
/// document fits its context window before reading it.
pub const MARKDOWN_TOKENS_HEADER: &str = "x-markdown-tokens";

/// Which representation of a page a request resolved to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Representation {
    /// The rendered HTML page — the default for anything that is not asking
    /// for Markdown specifically.
    Html,
    /// The page's Markdown source.
    Markdown,
}

/// The best `(q-value, list-index)` seen for each media range that matters
/// here, in one pass over the `Accept` header.
///
/// `q=0` entries are *kept* (as `Some((0.0, index))`): an explicit "not
/// acceptable" is different from a type going unmentioned, and [`resolve`]
/// depends on telling them apart.
///
/// [`resolve`]: MarkdownNegotiate::resolve
#[derive(Clone, Copy, Debug, Default)]
struct AcceptQualities {
    /// Best match for `text/html`.
    html: Option<(f32, usize)>,
    /// Best match for `text/markdown`.
    markdown: Option<(f32, usize)>,
    /// Best match for the `text/*` subtype wildcard, which covers *both*
    /// candidates and so can never favour one over the other.
    text_star: Option<(f32, usize)>,
    /// Best match for the `*/*` wildcard.
    wildcard: Option<(f32, usize)>,
}

/// Split a header list on `delimiter`, ignoring delimiters inside a quoted
/// string.
///
/// A media range may carry a parameter whose value is a quoted string, and a
/// quoted string may contain the very characters that separate ranges and
/// parameters (RFC 7230 §3.2.6). Splitting naively on `,` cuts
/// `text/markdown;profile="a,b";q=0` in half: the front half parses as
/// `text/markdown` with no `q` at all, i.e. `q=1`, turning the client's
/// *exclusion* into its strongest preference. Splitting naively on `;` inside
/// the entry has the same shape of bug for a value like `profile="a;q=1"`.
///
/// A backslash escapes the next character inside a quoted string (`quoted-pair`),
/// so `"a\",b"` is one value containing a quote and a comma, not the end of a
/// string. Cutting only on ASCII delimiters keeps the byte offsets valid slice
/// boundaries for UTF-8.
fn split_outside_quotes(list: &str, delimiter: u8) -> Vec<&str> {
    let mut entries = Vec::new();
    let mut start = 0;
    let mut quoted = false;
    let mut escaped = false;

    for (index, byte) in list.bytes().enumerate() {
        if escaped {
            escaped = false;
            continue;
        }

        match byte {
            b'\\' if quoted => escaped = true,
            b'"' => quoted = !quoted,
            _ if byte == delimiter && !quoted => {
                entries.push(&list[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }

    entries.push(&list[start..]);
    entries
}

/// Scan the request's `Accept` fields once, recording the best `(q, index)`
/// per range.
///
/// Media ranges and the quality parameter are matched case-insensitively
/// (RFC 7231 §3.1.1.1), and parameters other than `q` are ignored, so
/// `Accept: Text/Markdown;variant=GFM` records the Markdown slot at `q=1`.
/// Out-of-range q-values are clamped to `[0.0, 1.0]`.
///
/// Reads *every* `Accept` field, not just the first. A header that may appear
/// more than once is semantically one comma-separated list however it arrives
/// on the wire (RFC 7230 §3.2.2), and proxies do split and re-emit them. Taking
/// only `HeaderMap::get`'s first value would drop the rest, so `Accept: */*;q=0.1`
/// followed by `Accept: text/markdown` would resolve to HTML on the strength of
/// half the request. The index continues across fields, which is what makes the
/// tie-break ("earlier entry wins") mean the same thing in both wire forms.
fn accept_qualities(headers: &HeaderMap) -> AcceptQualities {
    let entries = headers
        .get_all(header::ACCEPT)
        .into_iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| split_outside_quotes(value, b','));

    let mut qualities = AcceptQualities::default();

    for (index, raw_entry) in entries.enumerate() {
        let entry = raw_entry.trim();
        if entry.is_empty() {
            continue;
        }

        let mut media_range = "";
        let mut quality = 1.0_f32;

        for (position, segment) in split_outside_quotes(entry, b';').into_iter().enumerate() {
            let segment = segment.trim();
            if position == 0 {
                media_range = segment;
                continue;
            }

            if let Some((name, value)) = segment.split_once('=')
                && name.trim().eq_ignore_ascii_case("q")
                && let Ok(parsed) = value.trim().parse::<f32>()
            {
                quality = parsed.clamp(0.0, 1.0);
            }
        }

        let slot = if media_range.eq_ignore_ascii_case("text/html") {
            &mut qualities.html
        } else if media_range.eq_ignore_ascii_case(MARKDOWN_MEDIA_TYPE) {
            &mut qualities.markdown
        } else if media_range.eq_ignore_ascii_case("text/*") {
            &mut qualities.text_star
        } else if media_range.eq_ignore_ascii_case("*/*") {
            &mut qualities.wildcard
        } else {
            continue;
        };

        if slot.is_none_or(|(recorded, _)| quality > recorded) {
            *slot = Some((quality, index));
        }
    }

    qualities
}

/// A resolved negotiation decision.
///
/// Unlike [`Representation`] this can say that the client forbade *both*
/// representations, which is a `406` rather than a page.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Resolution {
    Html,
    Markdown,
    NotAcceptable,
}

/// Extractor capturing a request's HTML-or-Markdown preference.
///
/// Named to stay distinct from `autumn_web::prelude`'s `Negotiate`, which is
/// glob-imported alongside it and negotiates a different pair (see the module
/// docs).
#[derive(Clone, Copy, Debug)]
pub struct MarkdownNegotiate {
    qualities: AcceptQualities,
}

impl MarkdownNegotiate {
    /// The representation this request resolved to.
    ///
    /// A lossy view: it cannot express "nothing is acceptable", collapsing that
    /// case onto [`Representation::Html`]. Use [`MarkdownNegotiate::respond`]
    /// for the full behaviour, which answers `406` there instead.
    #[must_use]
    pub fn representation(&self) -> Representation {
        match self.resolve() {
            Resolution::Markdown => Representation::Markdown,
            Resolution::Html | Resolution::NotAcceptable => Representation::Html,
        }
    }

    /// Apply the resolution policy documented at the module level.
    fn resolve(&self) -> Resolution {
        // Media-range precedence per RFC 7231 §5.3.2: `type/subtype` beats
        // `type/*` beats `*/*`. `.or()` picks the most specific range that is
        // present — and because a present range short-circuits even at
        // `Some(0.0)`, a specific `q=0` still forbids the type despite a
        // permissive wildcard further down the header.
        let html = self
            .qualities
            .html
            .or(self.qualities.text_star)
            .or(self.qualities.wildcard);
        let markdown = self
            .qualities
            .markdown
            .or(self.qualities.text_star)
            .or(self.qualities.wildcard);

        let html_forbidden = matches!(html, Some((quality, _)) if quality <= 0.0);
        let markdown_forbidden = matches!(markdown, Some((quality, _)) if quality <= 0.0);

        if html_forbidden && markdown_forbidden {
            return Resolution::NotAcceptable;
        }

        let html = html.filter(|&(quality, _)| quality > 0.0);
        // Markdown is *elected* only by its own media range, never by a
        // wildcard that merely covers it — see the module docs. A wildcard can
        // still forbid it (above), because an exclusion the client wrote is
        // not ours to reinterpret.
        let markdown = self
            .qualities
            .markdown
            .filter(|&(quality, _)| quality > 0.0);

        match (html, markdown) {
            (Some((html_q, html_index)), Some((markdown_q, markdown_index))) => {
                if (html_q - markdown_q).abs() < f32::EPSILON {
                    // Equal effective q: the earlier header entry wins.
                    match html_index.cmp(&markdown_index) {
                        std::cmp::Ordering::Greater => Resolution::Markdown,
                        std::cmp::Ordering::Less | std::cmp::Ordering::Equal => Resolution::Html,
                    }
                } else if markdown_q > html_q {
                    Resolution::Markdown
                } else {
                    Resolution::Html
                }
            }
            // Markdown was forbidden, or never named: HTML, the default.
            (Some(_), None) => Resolution::Html,
            // Markdown was named and HTML is forbidden or unmentioned.
            (None, Some(_)) => Resolution::Markdown,
            // Neither is a candidate. HTML unmentioned is HTML by default;
            // HTML *forbidden* with no Markdown named leaves nothing this
            // resource is willing to serve, and the `406` says which two
            // representations exist.
            (None, None) => {
                if html_forbidden {
                    Resolution::NotAcceptable
                } else {
                    Resolution::Html
                }
            }
        }
    }

    /// Serve `html` to browsers and `markdown` to agents, building only the one
    /// that was asked for.
    ///
    /// Both arms are closures because both are expensive: the HTML arm renders
    /// and syntax-highlights a guide, the Markdown arm assembles and copies its
    /// source. `406 Not Acceptable` runs neither.
    ///
    /// `Vary: Accept` is appended to every arm — including the `406` — so a
    /// cache that honours it keeps the representations apart. It is *appended*
    /// rather than inserted so the `Vary: accept-encoding` the compression
    /// layer adds later survives.
    pub fn respond<H, M>(self, html: H, markdown: M) -> Response
    where
        H: FnOnce() -> Response,
        M: FnOnce() -> MarkdownPage,
    {
        let mut response = match self.resolve() {
            Resolution::Html => html(),
            Resolution::Markdown => markdown().into_response(),
            Resolution::NotAcceptable => (
                StatusCode::NOT_ACCEPTABLE,
                [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
                "406 Not Acceptable: this URL serves text/html and text/markdown\n",
            )
                .into_response(),
        };

        response
            .headers_mut()
            .append(header::VARY, HeaderValue::from_static("Accept"));

        response
    }
}

/// Whether this request's `Accept` resolves to the Markdown representation.
///
/// For code that has the request headers but not the extractor — the
/// `Cache-Control` middleware, which must know which representation a response
/// belongs to *without* reading the response. A `304 Not Modified` from
/// [`EtagLayer`] is built fresh, carrying only `ETag`, so by the time it reaches
/// the middleware there is no `Content-Type` left to recognise Markdown by; and
/// a `304` is a `3xx`, so the page policy would otherwise claim it and mark a
/// revalidated Markdown response cacheable for an hour. The request is the one
/// thing that still says what was asked for.
///
/// Collapses `406` onto HTML like [`MarkdownNegotiate::representation`] does,
/// which is harmless here: the middleware gives a `406` no policy either way.
///
/// [`EtagLayer`]: autumn_web::etag::EtagLayer
#[must_use]
pub fn prefers_markdown(headers: &HeaderMap) -> bool {
    MarkdownNegotiate {
        qualities: accept_qualities(headers),
    }
    .representation()
        == Representation::Markdown
}

impl<S> FromRequestParts<S> for MarkdownNegotiate
where
    S: Send + Sync,
{
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        Ok(Self {
            qualities: accept_qualities(&parts.headers),
        })
    }
}

/// A Markdown representation of a page, with the status its HTML twin would
/// carry.
///
/// Responding sets `Content-Type: text/markdown; charset=utf-8` and
/// [`MARKDOWN_TOKENS_HEADER`]. It deliberately does *not* set `Vary` — the
/// negotiating [`MarkdownNegotiate::respond`] appends that to every arm, so
/// there is one place that decides it.
#[derive(Clone, Debug)]
pub struct MarkdownPage {
    status: StatusCode,
    body: String,
}

impl MarkdownPage {
    /// A `200 OK` Markdown page.
    #[must_use]
    pub const fn new(body: String) -> Self {
        Self {
            status: StatusCode::OK,
            body,
        }
    }

    /// A Markdown page answering with `status` — the Markdown twin of an
    /// HTML `404` or `500`, which an agent needs to see as the same failure a
    /// browser would.
    #[must_use]
    pub const fn with_status(status: StatusCode, body: String) -> Self {
        Self { status, body }
    }
}

impl IntoResponse for MarkdownPage {
    fn into_response(self) -> Response {
        let tokens = estimate_tokens(&self.body);

        let mut response = (
            self.status,
            [(header::CONTENT_TYPE, MARKDOWN_CONTENT_TYPE)],
            self.body,
        )
            .into_response();

        response.headers_mut().insert(
            MARKDOWN_TOKENS_HEADER,
            HeaderValue::from(u64::try_from(tokens).unwrap_or(u64::MAX)),
        );

        response
    }
}

/// Estimate how many tokens a Markdown body costs an LLM.
///
/// Four bytes per token is the usual rule of thumb for English prose and code
/// under a byte-pair encoder, and the header it feeds is explicitly an
/// estimate: it exists so a caller can tell a 2 KB guide from a 150 KB one
/// before reading it, not to be budgeted against exactly. Every tokenizer that
/// would answer this precisely is model-specific and would cost a pass over the
/// body per request; a length division costs nothing and is right to within the
/// order of magnitude the header is read for.
///
/// Counts bytes rather than characters, so multi-byte text over-estimates
/// slightly — the safe direction for a context-window check, and near-free
/// since `str::len` is already known.
#[must_use]
pub fn estimate_tokens(markdown: &str) -> usize {
    markdown.len().div_ceil(4)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What Chrome sends.
    const BROWSER_ACCEPT: &str =
        "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8";

    /// Build a [`MarkdownNegotiate`] as the extractor would, from an optional
    /// `Accept` header.
    fn negotiate(accept: Option<&str>) -> MarkdownNegotiate {
        negotiate_fields(accept.into_iter())
    }

    /// The same, for a request carrying more than one `Accept` field.
    fn negotiate_fields<'a>(accept: impl Iterator<Item = &'a str>) -> MarkdownNegotiate {
        let mut headers = HeaderMap::new();
        for value in accept {
            headers.append(header::ACCEPT, HeaderValue::from_str(value).unwrap());
        }

        MarkdownNegotiate {
            qualities: accept_qualities(&headers),
        }
    }

    #[test]
    fn a_preference_split_across_accept_fields_is_still_one_list() {
        // RFC 7230 §3.2.2: repeated fields are one comma-separated list. A proxy
        // that splits them must not cost the client its stated preference.
        assert_eq!(
            negotiate_fields(["*/*;q=0.1", "text/markdown"].into_iter()).representation(),
            Representation::Markdown,
        );
        assert_eq!(
            negotiate_fields(["text/html;q=0", "text/markdown;q=0"].into_iter()).resolve(),
            Resolution::NotAcceptable,
            "an exclusion in a later field forbids just as one in the first does",
        );
    }

    #[test]
    fn the_tie_break_reads_split_fields_in_arrival_order() {
        // Equal q across two fields: the earlier entry wins, and "earlier" has
        // to mean the same thing whether the list arrived in one field or two.
        assert_eq!(
            negotiate_fields(["text/markdown", "text/html"].into_iter()).representation(),
            Representation::Markdown,
        );
        assert_eq!(
            negotiate_fields(["text/html", "text/markdown"].into_iter()).representation(),
            Representation::Html,
        );
    }

    #[test]
    fn the_convention_header_serves_markdown() {
        assert_eq!(
            negotiate(Some("text/markdown")).representation(),
            Representation::Markdown,
        );
    }

    #[test]
    fn media_type_parameters_and_casing_do_not_hide_the_request() {
        // RFC 7231 §3.1.1.1: media ranges are case-insensitive, and a range may
        // carry parameters other than `q`.
        assert_eq!(
            negotiate(Some("Text/Markdown;variant=GFM")).representation(),
            Representation::Markdown,
        );
        assert_eq!(
            negotiate(Some("text/markdown;Q=1")).representation(),
            Representation::Markdown,
        );
    }

    #[test]
    fn a_browser_still_gets_html() {
        // What Chrome and Firefox actually send. `text/markdown` is only
        // reachable through `*/*;q=0.8`, which loses to `text/html` at q=1.
        assert_eq!(
            negotiate(Some(
                "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8"
            ))
            .representation(),
            Representation::Html,
        );
    }

    #[test]
    fn no_preference_gets_html() {
        // A bare `curl` sends `*/*`; a hand-rolled client may send nothing.
        assert_eq!(negotiate(None).representation(), Representation::Html);
        assert_eq!(
            negotiate(Some("*/*")).representation(),
            Representation::Html
        );
        assert_eq!(
            negotiate(Some("")).representation(),
            Representation::Html,
            "an empty Accept is no preference, not an empty set of preferences",
        );
    }

    #[test]
    fn the_text_wildcard_covers_both_and_so_prefers_neither() {
        // `text/*` names HTML and Markdown through the same entry, which is not
        // a preference between them: the default applies.
        assert_eq!(
            negotiate(Some("text/*")).representation(),
            Representation::Html,
        );
    }

    #[test]
    fn the_higher_quality_wins() {
        assert_eq!(
            negotiate(Some("text/html;q=0.8, text/markdown;q=1.0")).representation(),
            Representation::Markdown,
        );
        assert_eq!(
            negotiate(Some("text/markdown;q=0.5, text/html;q=0.9")).representation(),
            Representation::Html,
        );
    }

    #[test]
    fn an_exact_tie_goes_to_the_earlier_entry() {
        assert_eq!(
            negotiate(Some("text/markdown, text/html")).representation(),
            Representation::Markdown,
        );
        assert_eq!(
            negotiate(Some("text/html, text/markdown")).representation(),
            Representation::Html,
        );
    }

    #[test]
    fn a_named_markdown_beats_a_wildcard_covering_html() {
        // The shape an agent sends when it wants Markdown but will take
        // anything: `*/*` covers HTML at a lower q.
        assert_eq!(
            negotiate(Some("text/markdown, */*;q=0.1")).representation(),
            Representation::Markdown,
        );
    }

    #[test]
    fn a_wildcard_never_elects_markdown_on_its_own() {
        // A strict effective-q reading would serve Markdown here: `text/html`
        // is demoted to 0.1 while `*/*;q=1` covers Markdown at full quality.
        // Markdown is only ever elected by its own media range, so that the
        // edge's string match and the origin's decision cannot disagree — see
        // the module docs.
        assert_eq!(
            negotiate(Some("text/html;q=0.1, */*;q=1")).representation(),
            Representation::Html,
        );
        assert_eq!(
            negotiate(Some("text/*;q=1")).representation(),
            Representation::Html,
            "the subtype wildcard covers Markdown too, and elects it no more \
             than `*/*` does",
        );
    }

    #[test]
    fn a_specific_exclusion_survives_a_permissive_wildcard() {
        // `text/markdown;q=0` is more specific than `*/*;q=1`, so Markdown stays
        // forbidden and HTML is served.
        assert_eq!(
            negotiate(Some("text/markdown;q=0, */*;q=1")).resolve(),
            Resolution::Html,
        );
    }

    #[test]
    fn refusing_html_without_naming_markdown_is_not_acceptable() {
        // The client has refused the only representation it named, and a
        // wildcard does not elect the other one. Answering `406` says so;
        // answering HTML would serve what was explicitly refused, and answering
        // Markdown would be a decision the edge cannot mirror.
        assert_eq!(
            negotiate(Some("text/html;q=0")).resolve(),
            Resolution::NotAcceptable,
        );
        assert_eq!(
            negotiate(Some("text/html;q=0, */*;q=1")).resolve(),
            Resolution::NotAcceptable,
        );
        // Naming it is all it takes.
        assert_eq!(
            negotiate(Some("text/html;q=0, text/markdown")).resolve(),
            Resolution::Markdown,
        );
    }

    #[test]
    fn forbidding_every_representation_is_not_acceptable() {
        assert_eq!(
            negotiate(Some("text/html;q=0, text/markdown;q=0")).resolve(),
            Resolution::NotAcceptable,
        );
        assert_eq!(
            negotiate(Some("*/*;q=0")).resolve(),
            Resolution::NotAcceptable,
            "a blanket exclusion forbids both representations",
        );
        assert_eq!(
            negotiate(Some("text/*;q=0")).resolve(),
            Resolution::NotAcceptable,
            "the subtype wildcard covers both, so excluding it excludes both",
        );
    }

    #[test]
    fn an_unrelated_media_type_is_not_an_exclusion() {
        // `application/json` says nothing about either representation this
        // resource has, so the default applies rather than a 406.
        assert_eq!(
            negotiate(Some("application/json")).resolve(),
            Resolution::Html
        );
    }

    #[test]
    fn a_quoted_parameter_value_does_not_split_the_entry() {
        // `profile="a,b"` is one parameter containing a comma. Splitting on it
        // would leave `text/markdown;profile="a` with no `q`, i.e. q=1, and turn
        // this client's exclusion of Markdown into a request for it.
        assert_eq!(
            negotiate(Some(r#"text/markdown;profile="a,b";q=0, text/html;q=1"#)).resolve(),
            Resolution::Html,
        );
        // The same inside the entry: a quoted `;` is not a parameter boundary,
        // so the `q` here is the real one.
        assert_eq!(
            negotiate(Some(r#"text/markdown;profile="a;q=1";q=0, text/html"#)).resolve(),
            Resolution::Html,
        );
        // And a quoted delimiter must not hide a genuine preference either.
        assert_eq!(
            negotiate(Some(r#"text/markdown;profile="a,b", text/html;q=0.5"#)).representation(),
            Representation::Markdown,
        );
    }

    #[test]
    fn a_backslash_escapes_the_quote_it_precedes() {
        // `"a\",b"` is one quoted value holding a quote and a comma; reading the
        // escaped quote as the end of the string would split the entry there.
        assert_eq!(
            negotiate(Some(r#"text/markdown;profile="a\",b";q=0, text/html"#)).resolve(),
            Resolution::Html,
        );
    }

    #[test]
    fn out_of_range_qualities_are_clamped() {
        assert_eq!(
            negotiate(Some("text/html;q=9, text/markdown;q=1")).representation(),
            Representation::Html,
            "an over-range q clamps to 1.0 rather than winning outright",
        );
        assert_eq!(
            negotiate(Some("text/markdown;q=-1, text/html")).resolve(),
            Resolution::Html,
            "a negative q clamps to 0.0, which is an exclusion",
        );
    }

    #[test]
    fn the_best_quality_for_a_repeated_range_is_the_one_kept() {
        assert_eq!(
            negotiate(Some(
                "text/markdown;q=0.1, text/markdown;q=1, text/html;q=0.5"
            ))
            .representation(),
            Representation::Markdown,
        );
    }

    #[test]
    fn the_middlewares_view_of_a_request_matches_the_extractors() {
        // `prefers_markdown` decides the cache policy of responses whose
        // `Content-Type` is gone (a 304), so it must not drift from the
        // resolution the handler used to build the body in the first place.
        for accept in [
            "text/markdown",
            "text/markdown, */*;q=0.1",
            "text/html, text/markdown",
            "text/html;q=0.5, text/markdown",
            "text/html;q=0",
            "*/*",
            "text/*",
            BROWSER_ACCEPT,
        ] {
            let mut headers = HeaderMap::new();
            headers.insert(header::ACCEPT, HeaderValue::from_str(accept).unwrap());

            assert_eq!(
                prefers_markdown(&headers),
                negotiate(Some(accept)).representation() == Representation::Markdown,
                "the middleware and the extractor disagree about {accept}",
            );
        }

        assert!(
            !prefers_markdown(&HeaderMap::new()),
            "a request with no Accept is an HTML request",
        );
    }

    #[test]
    fn token_estimates_scale_with_length_and_never_round_to_zero() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(
            estimate_tokens("a"),
            1,
            "a non-empty body costs at least one"
        );
        assert_eq!(estimate_tokens("abcd"), 1);
        assert_eq!(estimate_tokens("abcde"), 2);
    }
}

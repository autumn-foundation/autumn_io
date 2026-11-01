use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use autumn_io::docs::slugify_heading;
use pulldown_cmark::{Event, Options, Parser, Tag};
use regex::Regex;

const DEFAULT_AUTUMN_REPO: &str = "../autumn";
const DEFAULT_DESTINATION: &str = "content/guide";
const SOURCE_ENV: &str = "AUTUMN_REPO_DIR";

/// The GitHub org was renamed from `madmax983` to `autumn-foundation`.
/// Upstream guide markdown still references the old org, so rewrite every
/// occurrence when vendoring so the vendored snapshot stays on the new org.
const OLD_ORG_PREFIX: &str = "github.com/madmax983/";
const NEW_ORG_PREFIX: &str = "github.com/autumn-foundation/";

/// Rewrite upstream GitHub org references onto the renamed `autumn-foundation`
/// org. Applied to every guide body before it is written.
fn rewrite_org_urls(markdown: &str) -> String {
    markdown.replace(OLD_ORG_PREFIX, NEW_ORG_PREFIX)
}

const GUIDE_FILES: &[(&str, u32)] = &[
    ("getting-started.md", 10),
    ("what-happens-when.md", 20),
    ("coming-from-other-frameworks.md", 30),
    ("accessibility.md", 40),
    ("middleware.md", 50),
    ("path-helpers.md", 60),
    ("routes-cli.md", 70),
    ("macro-transparency.md", 80),
    ("testing.md", 90),
    ("transactions.md", 100),
    ("seeding.md", 110),
    ("storage.md", 120),
    ("mail.md", 130),
    ("authorization.md", 140),
    ("signed-webhooks.md", 150),
    ("signing-secrets.md", 160),
    ("realtime.md", 170),
    ("websockets.md", 180),
    ("jobs.md", 190),
    ("tasks.md", 200),
    ("operating-background-jobs.md", 210),
    ("scheduled-multi-replica.md", 220),
    ("admin.md", 230),
    ("generators.md", 240),
    ("custom-subsystems.md", 250),
    ("extensibility.md", 260),
    ("cloud-native.md", 270),
    ("deployment.md", 280),
    ("docs-smoke.md", 290),
    ("i18n.md", 300),
    // New in Autumn 0.5.0. Order weights continue the existing scheme (tens),
    // grouped by the sidebar clusters they are wired into so registry sort
    // tracks navigation order.
    ("compression.md", 310),
    ("conditional-get.md", 320),
    ("pagination.md", 330),
    ("active-search-and-autocomplete.md", 340),
    ("wizards.md", 350),
    ("hooks-and-transactions.md", 360),
    ("repositories.md", 370),
    ("migrations.md", 380),
    ("soft-delete.md", 390),
    ("state-machines.md", 400),
    ("version-history.md", 410),
    ("full-text-search.md", 420),
    ("storage-variants.md", 430),
    ("attribute-encryption.md", 440),
    ("oauth.md", 450),
    ("step-up-authentication.md", 460),
    ("credentials.md", 470),
    ("bot-protection.md", 480),
    ("idempotency.md", 490),
    ("logging-pii.md", 500),
    ("presence.md", 510),
    ("api-versioning.md", 520),
    ("outbound-http.md", 530),
    ("outbound-webhooks.md", 540),
    ("mcp.md", 550),
    ("feature-flags.md", 560),
    ("experiments.md", 570),
    ("runtime-config.md", 580),
    ("resilience.md", 590),
    ("health-indicators.md", 600),
    ("metrics-sources.md", 610),
    ("error-reporting.md", 620),
    ("maintenance-mode.md", 630),
    ("staged-deploys.md", 640),
    ("dev-error-overlay.md", 650),
    ("dev-inspector.md", 660),
    ("dev-loop-latency.md", 670),
    ("system-tests.md", 680),
    // New in Autumn 0.6.0. Order weights continue the tens scheme, grouped by
    // the sidebar clusters they are wired into so registry sort tracks the
    // navigation order.
    ("flash.md", 690),
    ("tabs.md", 700),
    ("declarative-schema.md", 710),
    ("events.md", 720),
    ("lifecycle.md", 730),
    ("mail-compliance.md", 740),
    ("cache-stampede.md", 750),
    ("daemon.md", 760),
    ("distributed-locks.md", 770),
    ("fragment-caching.md", 780),
    ("operator-alerts.md", 790),
    ("rate-limiting.md", 800),
    ("security-posture-manifest.md", 810),
    ("tls.md", 820),
    ("format-helpers.md", 830),
    ("stories.md", 840),
    ("time-zones.md", 850),
    ("transition-effects.md", 860),
    ("wasm-islands.md", 870),
    ("widget-styling.md", 880),
    ("sharding.md", 890),
    ("sqlite-in-production.md", 900),
    ("tenant-cells.md", 910),
    ("tauri.md", 920),
    ("tauri-mobile-in-process.md", 930),
    ("tauri-mobile-offline-sync.md", 940),
    ("tauri-mobile-thin-client.md", 950),
    ("starters.md", 960),
    // New guides folded in after the 0.6.0 sync. Weights continue the trailing
    // tens block (the established append-as-batch pattern); the sidebar order
    // itself is governed by `DOCS_NAV_GROUPS` in `site.rs`.
    ("submit-tokens.md", 970),
    ("downloads.md", 980),
    ("media.md", 990),
    // Two newer upstream guides folded in alongside the Harvest 0.5 sync.
    ("content-negotiation.md", 1000),
    ("nested-forms.md", 1010),
    // New in Autumn 0.7.0. Weights continue the trailing tens block, grouped by
    // the sidebar clusters they are wired into so registry sort tracks the
    // navigation order. Entries may be nested under `docs/guide/`; the site slug
    // is always the file stem, so `observability/server-timing.md` is served at
    // `/docs/server-timing`.
    ("seo.md", 1150),
    ("pdf-downloads.md", 1160),
    ("rich-text.md", 1170),
    ("commentable.md", 1180),
    ("votable.md", 1190),
    ("feeds.md", 1200),
    ("notifications.md", 1210),
    ("search.md", 1220),
    ("openapi.md", 1230),
    ("authentication.md", 1240),
    ("route-auth-coverage.md", 1250),
    ("aggregates.md", 1260),
    ("counter-cache.md", 1270),
    // `ledgered-entities.md` and `query-budgets.md` were held back from the
    // 0.7.0 sync because upstream `trunk-dev` documented APIs (`ledgered =
    // true`, `#[query_budget(N)]`) the published 0.7.0 crates lacked. Both
    // ship in autumn-web/autumn-macros 0.8.0, so they fill the weights that
    // were left open for them.
    ("ledgered-entities.md", 1280),
    ("audit-logging.md", 1290),
    ("retention-sweeps.md", 1300),
    ("query-budgets.md", 1310),
    ("metrics.md", 1320),
    ("observability/server-timing.md", 1330),
    ("failure-capsules.md", 1340),
    ("console.md", 1350),
    ("simulation-testing.md", 1360),
    ("clustering.md", 1370),
    ("upgrading.md", 1380),
    ("edge.md", 1390),
    ("fleet-deploys.md", 1400),
    // New in Autumn 0.8.0. Weights continue the trailing tens block, grouped by
    // the sidebar clusters they are wired into. Synced from the `v0.8.0` tag,
    // not `trunk-dev`, so every page documents an API the published crates
    // carry. Upstream's `index.md` is not vendored: it is a hand-kept link
    // list the sidebar already replaces, and it links the `tutorial/`
    // sequence, which stays unvendored while six of its twelve chapters are
    // still signposts back into Getting Started.
    ("platform-support.md", 1410),
    ("extractors.md", 1420),
    ("forms.md", 1430),
    ("cors.md", 1440),
    ("collaboration.md", 1450),
    ("derivations.md", 1460),
    ("cache-coherence.md", 1470),
    ("money.md", 1480),
    ("web-push.md", 1490),
    ("billing.md", 1500),
    ("wire-contracts.md", 1510),
    ("agent-authority.md", 1520),
    ("posture-gate.md", 1530),
    ("supply-chain.md", 1540),
    ("confidential-fields.md", 1550),
    ("data-classification.md", 1560),
    ("data-retention.md", 1570),
    ("data-scrubbing.md", 1580),
    ("cookie-consent.md", 1590),
    ("capacity-contracts.md", 1600),
    ("sla.md", 1610),
    ("hot-upgrades.md", 1620),
    ("architecture-graph.md", 1630),
    ("constela.md", 1640),
    ("plugin-assets.md", 1650),
    ("sandboxed-plugins.md", 1660),
    // New in Autumn 0.9.0. Synced from the `v0.9.0` tag. The tutorial and
    // `index.md` stay unvendored: the tutorial chapters are still outlines.
    ("aeo.md", 1670),
    ("webmcp.md", 1680),
    ("connection-poolers.md", 1690),
    ("outbox.md", 1700),
    ("data-capsules.md", 1710),
    ("timeouts-and-budgets.md", 1720),
    ("connection-limits.md", 1730),
    ("observability/overload-signals.md", 1740),
    ("slo.md", 1750),
    ("fault-injection.md", 1760),
    ("cost.md", 1770),
    ("cell-isolation.md", 1780),
    ("sqlite-fleet.md", 1790),
    ("kubernetes.md", 1800),
];

const DEFAULT_HARVEST_REPO: &str = "../autumn-harvest";
const HARVEST_SOURCE_ENV: &str = "AUTUMN_HARVEST_REPO_DIR";

/// The `autumn-harvest` repository the guide chapters cross-link back into for
/// files the site does not vendor (the reference examples, the management-API
/// spec, and so on). Chapter cross-links are resolved to absolute URLs against
/// this repo/branch at sync time so the runtime link rewriter — which only
/// knows the framework repo — leaves them untouched.
const HARVEST_REPOSITORY_URL: &str = "https://github.com/autumn-foundation/autumn-harvest";
const HARVEST_REPOSITORY_BRANCH: &str = "trunk-dev";

/// The site slug the hand-authored Harvest section intro is served at. The
/// upstream guide's `README.md` index links resolve here.
const HARVEST_INTRO_SLUG: &str = "autumn-harvest";

/// Harvest's own guide — the `docs/getting-started/` chapter sequence — folded
/// into this site. Each entry maps an upstream chapter file to the site slug it
/// is served at and its registry `order`. Slugs carry a `harvest-` prefix so
/// they share the guide namespace without colliding with the framework guides;
/// the sidebar home is the "Harvest" group in `site.rs`, anchored by the
/// `autumn-harvest` intro. The orphaned `activities.md` reference (not part of
/// the upstream chapter flow) is intentionally not vendored.
const HARVEST_GUIDE_FILES: &[(&str, &str, u32)] = &[
    ("01-project-skeleton.md", "harvest-project-skeleton", 1020),
    ("02-first-workflow.md", "harvest-first-workflow", 1030),
    // New in Harvest 0.7.0: a fork off chapter 2 for services on plain Axum
    // rather than autumn-web, so it sits between chapters 2 and 3.
    ("standalone-axum.md", "harvest-standalone-axum", 1035),
    ("03-durable-timers.md", "harvest-durable-timers", 1040),
    ("04-signals.md", "harvest-signals", 1050),
    ("05-child-workflows.md", "harvest-child-workflows", 1060),
    ("06-idempotency.md", "harvest-idempotency", 1070),
    ("07-reliability-knobs.md", "harvest-reliability-knobs", 1080),
    (
        "08-dags-and-schedules.md",
        "harvest-dags-and-schedules",
        1090,
    ),
    ("09-worker-routing.md", "harvest-worker-routing", 1100),
    ("10-operations.md", "harvest-operations", 1110),
    ("11-testing.md", "harvest-testing", 1120),
    ("12-webhooks.md", "harvest-webhooks", 1130),
    // New in Harvest 0.6.0.
    ("13-broker-connectors.md", "harvest-broker-connectors", 1140),
];

/// Harvest's topic docs (`docs/*.md` in autumn-harvest): the reference for each
/// feature, outside the chapter sequence. Each entry maps an upstream file to
/// its site slug and registry `order`. The sidebar groups them in `site.rs`.
///
/// Internal engineering records stay upstream: the architecture documents,
/// `shipped-work.md`, the benchmark and performance notes, and the safety
/// report.
const HARVEST_REFERENCE_FILES: &[(&str, &str, u32)] = &[
    // Agents and AI.
    ("agent-adapter.md", "harvest-durable-agents", 1200),
    ("agent-mesh.md", "harvest-agent-mesh", 1210),
    ("llm-cost-ledger.md", "harvest-agent-cost-ledger", 1220),
    ("mcp-tools.md", "harvest-mcp-tools", 1230),
    ("remote-tasks.md", "harvest-remote-tasks", 1240),
    ("sandbox-activities.md", "harvest-sandbox-activities", 1250),
    // Workflow patterns.
    ("durable-routes.md", "harvest-durable-routes", 1300),
    ("durable-promises.md", "harvest-durable-promises", 1310),
    ("saga.md", "harvest-saga", 1320),
    (
        "transactional-start.md",
        "harvest-transactional-start",
        1330,
    ),
    (
        "transactional-activities.md",
        "harvest-transactional-activities",
        1340,
    ),
    (
        "typed-workflow-failures.md",
        "harvest-typed-workflow-failures",
        1350,
    ),
    (
        "completion-callbacks.md",
        "harvest-completion-callbacks",
        1360,
    ),
    (
        "completion-triggers.md",
        "harvest-completion-triggers",
        1370,
    ),
    ("calendars.md", "harvest-calendars", 1380),
    ("search-attributes.md", "harvest-search-attributes", 1390),
    ("streaming-progress.md", "harvest-streaming-progress", 1400),
    ("workflow-logs.md", "harvest-workflow-logs", 1410),
    ("logical-handle.md", "harvest-logical-handle", 1420),
    (
        "decision-boundaries.md",
        "harvest-decision-boundaries",
        1430,
    ),
    ("sticky-routing.md", "harvest-sticky-routing", 1440),
    ("fairness-keys.md", "harvest-fairness-keys", 1450),
    ("retry-jitter.md", "harvest-retry-jitter", 1460),
    ("embedding.md", "harvest-embedding", 1470),
    ("sqlite-backend.md", "harvest-sqlite-backend", 1480),
    // Determinism and replay.
    (
        "why-deterministic-replay.md",
        "harvest-why-deterministic-replay",
        1500,
    ),
    (
        "workflow-determinism-guide.md",
        "harvest-workflow-determinism",
        1510,
    ),
    (
        "workflow-schema-contract-guide.md",
        "harvest-workflow-schema-contract",
        1520,
    ),
    ("harvest-verify.md", "harvest-verify", 1530),
    ("replay-verify.md", "harvest-replay-verify", 1540),
    ("replay-drift-gate.md", "harvest-replay-drift-gate", 1550),
    ("replay-debugger.md", "harvest-replay-debugger", 1560),
    ("upgrade-check.md", "harvest-upgrade-check", 1570),
    ("workflow-reset.md", "harvest-workflow-reset", 1580),
    // Operations.
    ("management-api.md", "harvest-management-api", 1600),
    ("openapi.md", "harvest-openapi", 1610),
    ("api-contract-guide.md", "harvest-api-contract", 1620),
    ("vantage-ui.md", "harvest-vantage-ui", 1630),
    ("operator-role.md", "harvest-operator-role", 1640),
    ("security-posture.md", "harvest-security-posture", 1650),
    ("telemetry.md", "harvest-telemetry", 1660),
    ("sharding.md", "harvest-sharding", 1670),
    ("partitioned-events.md", "harvest-partitioned-events", 1680),
    ("archival.md", "harvest-archival", 1690),
    ("audit-export.md", "harvest-audit-export", 1700),
    ("cross-region-dr.md", "harvest-cross-region-dr", 1710),
    // Adoption.
    ("comparison.md", "harvest-comparison", 1800),
    (
        "migrating-from-temporal.md",
        "harvest-migrating-from-temporal",
        1810,
    ),
];

/// A guide read from upstream and massaged for the site, held before writing so
/// the whole set's headings are known — [`normalize_fragments`] needs the target
/// page's headings to resolve a cross-page fragment.
struct PreparedGuide {
    slug: String,
    title: String,
    description: String,
    order: u32,
    body: String,
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse(env::args_os().skip(1))?;
    let source = guide_source_dir(&args.source)?;

    let mut guides = Vec::new();
    for (file_name, order) in GUIDE_FILES {
        let source_file = source.join(file_name);
        let raw = fs::read_to_string(&source_file)
            .map_err(|error| format!("failed to read {}: {error}", source_file.display()))?;
        let raw = strip_issue_references(&rebase_nested_links(&rewrite_org_urls(&raw), file_name));
        let title = first_heading(&raw).ok_or_else(|| {
            format!(
                "guide file {} must start with a level-one heading",
                source_file.display()
            )
        })?;
        let description = first_description(&raw).unwrap_or_else(|| title.clone());

        guides.push(PreparedGuide {
            slug: guide_slug(file_name).to_owned(),
            title,
            description,
            order: *order,
            body: raw,
        });
    }

    guides.extend(prepare_harvest_guide(&args.harvest_source)?);

    let headings: HashMap<&str, Vec<String>> = guides
        .iter()
        .map(|guide| (guide.slug.as_str(), headings(&guide.body)))
        .collect();

    fs::create_dir_all(&args.destination)?;
    for guide in &guides {
        let body = clamp_heading_levels(&normalize_fragments(&guide.body, &guide.slug, &headings));
        write_guide(
            &args.destination,
            &guide.slug,
            &guide.title,
            &guide.description,
            &guide.order,
            &body,
        )?;
    }

    Ok(())
}

/// Fold Harvest's own guide (the `docs/getting-started/` chapter sequence) into
/// the vendored snapshot. The chapters are massaged for the site: verbose
/// `Chapter N — …` headings become clean nav titles, the redundant top/bottom
/// chapter-nav chrome is stripped (the site supplies its own prev/next), and
/// every relative link is resolved — sibling chapters to their `/docs/harvest-*`
/// route, the guide index to the Harvest section intro, and everything else to
/// an absolute `autumn-harvest` source URL.
fn prepare_harvest_guide(harvest_source: &Path) -> Result<Vec<PreparedGuide>, Box<dyn Error>> {
    let source = harvest_guide_source_dir(harvest_source)?;
    let mut guides = Vec::new();

    for (file_name, slug, order) in HARVEST_GUIDE_FILES {
        let source_file = source.join(file_name);
        let raw = fs::read_to_string(&source_file)
            .map_err(|error| format!("failed to read {}: {error}", source_file.display()))?;
        let raw = strip_issue_references(&rewrite_org_urls(&raw));
        let raw = rewrite_harvest_links(&raw, HARVEST_GUIDE_DIR);

        let heading = first_heading(&raw).ok_or_else(|| {
            format!(
                "harvest guide file {} must start with a level-one heading",
                source_file.display()
            )
        })?;
        let title = clean_harvest_title(&heading);
        let body = prepare_harvest_body(&raw, &title);
        let description = first_description(&body).unwrap_or_else(|| title.clone());

        guides.push(PreparedGuide {
            slug: (*slug).to_owned(),
            title,
            description,
            order: *order,
            body,
        });
    }

    let reference = source
        .parent()
        .ok_or("the harvest guide directory has no parent `docs/` directory")?;
    for (file_name, slug, order) in HARVEST_REFERENCE_FILES {
        let source_file = reference.join(file_name);
        let raw = fs::read_to_string(&source_file)
            .map_err(|error| format!("failed to read {}: {error}", source_file.display()))?;
        let raw = strip_issue_references(&rewrite_harvest_links(
            &rewrite_org_urls(&raw),
            HARVEST_DOCS_DIR,
        ));

        let heading = first_heading(&raw).ok_or_else(|| {
            format!(
                "harvest reference file {} must start with a level-one heading",
                source_file.display()
            )
        })?;
        let title = clean_reference_title(&heading);
        let body = replace_first_heading(&raw, &title);
        let description = first_description(&body).unwrap_or_else(|| title.clone());

        guides.push(PreparedGuide {
            slug: (*slug).to_owned(),
            title,
            description,
            order: *order,
            body,
        });
    }

    Ok(guides)
}

/// Make a sidebar label from a Harvest reference heading. The label drops
/// the upstream issue number (`Calendar-Aware Schedules (issue #337)`), the
/// subtitle after an em dash (`MCP Tools — expose …`) and code backticks,
/// which a plain-text label shows as literal characters.
fn clean_reference_title(heading: &str) -> String {
    let mut title = heading.trim();
    if let Some(open) = title.rfind(" (issue")
        && title.ends_with(')')
    {
        title = title[..open].trim_end();
    }
    if let Some((head, _)) = title.split_once(" — ") {
        title = head.trim_end();
    }
    title.replace('`', "")
}

/// Set the first level-one heading to `title`, so that the site's
/// redundant-title step removes it.
fn replace_first_heading(markdown: &str, title: &str) -> String {
    let mut replaced = false;
    let mut output = String::with_capacity(markdown.len());
    for line in markdown.lines() {
        if !replaced && line.starts_with("# ") {
            output.push_str("# ");
            output.push_str(title);
            replaced = true;
        } else {
            output.push_str(line);
        }
        output.push('\n');
    }
    output
}

fn write_guide(
    destination: &Path,
    slug: &str,
    title: &str,
    description: &str,
    order: &u32,
    body: &str,
) -> Result<(), Box<dyn Error>> {
    let output = format!(
        "+++\ntitle = {}\ndescription = {}\norder = {order}\n+++\n\n{}",
        toml_string(title),
        toml_string(description),
        body.trim_start()
    );
    fs::write(destination.join(format!("{slug}.md")), output)?;
    Ok(())
}

/// Strip the verbose `Chapter N — `/`Chapter N: ` prefix from a Harvest chapter
/// heading, leaving a clean sidebar/title label (`Project skeleton`). Harvest
/// 0.7.0's side chapter is labelled `Fork — …` instead of numbered; that label
/// is stripped the same way, since the sidebar already places it after
/// chapter 2.
fn clean_harvest_title(heading: &str) -> String {
    let heading = heading.trim();
    if let Some(rest) = heading.strip_prefix("Chapter ") {
        let after_number = rest.trim_start_matches(|char: char| char.is_ascii_digit());
        if let Some(cleaned) = strip_title_separator(after_number) {
            return cleaned.to_owned();
        }
    }
    if let Some(rest) = heading.strip_prefix("Fork")
        && rest.trim_start().starts_with(['—', ':', '-'])
        && let Some(cleaned) = strip_title_separator(rest)
    {
        return cleaned.to_owned();
    }
    heading.to_owned()
}

/// What follows a chapter label's `—`/`:`/`-` separator, or `None` if nothing
/// does.
fn strip_title_separator(rest: &str) -> Option<&str> {
    let cleaned = rest.trim_start().trim_start_matches(['—', ':', '-']).trim();
    (!cleaned.is_empty()).then_some(cleaned)
}

/// Rewrite the H1 to the cleaned title (so the site's redundant-title stripping
/// removes it) and drop the redundant chapter-nav chrome: the `[← …] · [Next …]`
/// lines plus the top and bottom thematic-break dividers that framed them.
fn prepare_harvest_body(markdown: &str, title: &str) -> String {
    let mut lines: Vec<String> = markdown
        .lines()
        .filter(|line| !is_chapter_nav_line(line))
        .map(str::to_owned)
        .collect();

    if let Some(heading) = lines.iter_mut().find(|line| line.starts_with("# ")) {
        *heading = format!("# {title}");
    }

    strip_divider_after_heading(&mut lines);
    strip_trailing_divider(&mut lines);

    lines.join("\n")
}

/// A chapter-nav line is the upstream `[← Prev](…) · [Index](…) · [Next →](…)`
/// chrome: a link line carrying a `←`/`→` arrow. Prose that merely mentions an
/// arrow (`RUNNING → COMPLETED`) is not a link line and is kept.
fn is_chapter_nav_line(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with('[')
        && trimmed.contains("](")
        && (trimmed.contains('←') || trimmed.contains('→'))
}

/// Remove a `---` divider that sits directly under the H1 (once the nav line it
/// framed is gone), so the article does not open on a stray horizontal rule.
fn strip_divider_after_heading(lines: &mut Vec<String>) {
    let Some(heading_index) = lines.iter().position(|line| line.starts_with("# ")) else {
        return;
    };
    let divider = lines
        .iter()
        .enumerate()
        .skip(heading_index + 1)
        .find(|(_, line)| !line.trim().is_empty());
    if let Some((index, line)) = divider
        && line.trim() == "---"
    {
        lines.remove(index);
    }
}

/// Remove a trailing `---` divider left dangling once the bottom nav line is
/// gone.
fn strip_trailing_divider(lines: &mut Vec<String>) {
    let last = lines
        .iter()
        .enumerate()
        .rev()
        .find(|(_, line)| !line.trim().is_empty());
    if let Some((index, line)) = last
        && line.trim() == "---"
    {
        lines.remove(index);
    }
}

/// Rewrite the relative links inside a Harvest chapter. Sibling chapters map to
/// their site route, the guide index maps to the Harvest intro, and every other
/// relative path resolves to an absolute `autumn-harvest` source URL (so the
/// runtime rewriter, which only knows the framework repo, leaves it alone).
fn rewrite_harvest_links(markdown: &str, base: &str) -> String {
    rewrite_links(markdown, |destination| {
        rewrite_harvest_destination(destination, base)
    })
}

/// Rewrite the destination of each inline Markdown link outside fenced code.
fn rewrite_links(markdown: &str, rewrite: impl Fn(&str) -> String) -> String {
    let mut output = String::with_capacity(markdown.len());
    let mut in_fence = false;

    for line in markdown.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            output.push_str(line);
            output.push('\n');
            continue;
        }
        if in_fence {
            output.push_str(line);
            output.push('\n');
            continue;
        }

        match rewrite_reference_definition(line, &rewrite) {
            Some(rewritten) => output.push_str(&rewritten),
            None => output.push_str(&rewrite_links_in_line(line, &rewrite)),
        }
        output.push('\n');
    }

    output
}

/// Rewrite the destination of a link reference definition
/// (`[label]: ./replay-verify.md "title"`), or return `None` when `line` is
/// not one.
fn rewrite_reference_definition(line: &str, rewrite: &impl Fn(&str) -> String) -> Option<String> {
    let indent = line.len() - line.trim_start().len();
    let rest = line[indent..].strip_prefix('[')?;
    let close = rest.find("]:")?;
    if close == 0 || indent > 3 {
        return None;
    }
    let after = &rest[close + 2..];
    let destination_start = after.len() - after.trim_start().len();
    let destination = after[destination_start..].split_whitespace().next()?;
    let tail = &after[destination_start + destination.len()..];
    Some(format!(
        "{}[{}]:{}{}{tail}",
        &line[..indent],
        &rest[..close],
        &after[..destination_start],
        rewrite(destination),
    ))
}

fn rewrite_links_in_line(line: &str, rewrite: &impl Fn(&str) -> String) -> String {
    let mut output = String::with_capacity(line.len());
    let mut rest = line;

    while let Some(open) = rest.find("](") {
        let after = &rest[open + 2..];
        let Some(close) = after.find(')') else {
            break;
        };
        output.push_str(&rest[..open + 2]);
        output.push_str(&rewrite(&after[..close]));
        output.push(')');
        rest = &after[close + 1..];
    }

    output.push_str(rest);
    output
}

/// The repository directory of the Harvest chapters.
const HARVEST_GUIDE_DIR: &str = "docs/getting-started";
/// The repository directory of the Harvest reference docs.
const HARVEST_DOCS_DIR: &str = "docs";

fn rewrite_harvest_destination(destination: &str, base: &str) -> String {
    let destination = destination.trim();
    let lower = destination.to_ascii_lowercase();
    if destination.is_empty()
        || destination.starts_with('#')
        || lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("mailto:")
        || lower.starts_with("tel:")
    {
        return destination.to_owned();
    }

    let (path, fragment) = destination
        .split_once('#')
        .map_or((destination, String::new()), |(path, fragment)| {
            (path, format!("#{fragment}"))
        });

    let resolved = resolve_harvest_source_path(path, base);
    if let Some(slug) = harvest_site_slug(&resolved) {
        return format!("/docs/{slug}{fragment}");
    }

    let mode = if resolved
        .rsplit('/')
        .next()
        .is_some_and(|last| last.contains('.'))
    {
        "blob"
    } else {
        "tree"
    };
    format!("{HARVEST_REPOSITORY_URL}/{mode}/{HARVEST_REPOSITORY_BRANCH}/{resolved}{fragment}")
}

/// The site slug that serves a repository-root Harvest path, if the site
/// vendors that file.
fn harvest_site_slug(resolved: &str) -> Option<&'static str> {
    if let Some(file_name) = resolved
        .strip_prefix(HARVEST_GUIDE_DIR)
        .and_then(|rest| rest.strip_prefix('/'))
    {
        if file_name == "README.md" {
            return Some(HARVEST_INTRO_SLUG);
        }
        return HARVEST_GUIDE_FILES
            .iter()
            .find(|(name, _, _)| *name == file_name)
            .map(|(_, slug, _)| *slug);
    }
    let file_name = resolved
        .strip_prefix(HARVEST_DOCS_DIR)
        .and_then(|rest| rest.strip_prefix('/'))?;
    HARVEST_REFERENCE_FILES
        .iter()
        .find(|(name, _, _)| *name == file_name)
        .map(|(_, slug, _)| *slug)
}

/// Resolve a path relative to `base` (a repository directory) into a
/// repository-root path for an absolute `autumn-harvest` source link.
fn resolve_harvest_source_path(path: &str, base: &str) -> String {
    let mut segments: Vec<&str> = base.split('/').collect();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            segment => segments.push(segment),
        }
    }
    segments.join("/")
}

/// Rebase the relative links of a guide nested under `docs/guide/` (for
/// example `observability/`). The site serves every guide at its file stem,
/// and the renderer resolves relative links from `docs/guide/`. A link such as
/// `../metrics.md` in `observability/overload-signals.md` must therefore
/// become `metrics.md`.
fn rebase_nested_links(markdown: &str, file_name: &str) -> String {
    match file_name.rsplit_once('/') {
        Some((dir, _)) => rewrite_links(markdown, |destination| {
            rebase_nested_destination(destination, dir)
        }),
        None => markdown.to_owned(),
    }
}

fn rebase_nested_destination(destination: &str, dir: &str) -> String {
    let destination = destination.trim();
    let lower = destination.to_ascii_lowercase();
    if destination.is_empty()
        || destination.starts_with('#')
        || destination.starts_with('/')
        || lower.contains("://")
        || lower.starts_with("mailto:")
        || lower.starts_with("tel:")
    {
        return destination.to_owned();
    }

    let (path, fragment) = destination
        .split_once('#')
        .map_or((destination, ""), |(path, _)| {
            (path, &destination[path.len()..])
        });
    let mut segments: Vec<&str> = dir.split('/').filter(|s| !s.is_empty()).collect();
    let mut above = 0_usize;
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                if segments.pop().is_none() {
                    above += 1;
                }
            }
            segment => segments.push(segment),
        }
    }
    let mut rebased = "../".repeat(above);
    rebased.push_str(&segments.join("/"));
    rebased.push_str(fragment);
    rebased
}

fn harvest_guide_source_dir(source: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let nested = source.join("docs").join("getting-started");
    if nested.is_dir() {
        return Ok(nested);
    }

    if source.join("01-project-skeleton.md").is_file() {
        return Ok(source.to_owned());
    }

    Err(format!(
        "{} is neither an autumn-harvest repo root nor a docs/getting-started directory",
        source.display()
    )
    .into())
}

struct Args {
    source: PathBuf,
    harvest_source: PathBuf,
    destination: PathBuf,
}

impl Args {
    fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Self, Box<dyn Error>> {
        let mut source = env::var_os(SOURCE_ENV)
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(DEFAULT_AUTUMN_REPO));
        let mut harvest_source = env::var_os(HARVEST_SOURCE_ENV)
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(DEFAULT_HARVEST_REPO));
        let mut destination = PathBuf::from(DEFAULT_DESTINATION);

        let mut args = args.into_iter();
        while let Some(arg) = args.next() {
            match arg.to_string_lossy().as_ref() {
                "--source" => {
                    source = args
                        .next()
                        .map(PathBuf::from)
                        .ok_or("--source needs a path")?;
                }
                "--harvest-source" => {
                    harvest_source = args
                        .next()
                        .map(PathBuf::from)
                        .ok_or("--harvest-source needs a path")?;
                }
                "--dest" => {
                    destination = args
                        .next()
                        .map(PathBuf::from)
                        .ok_or("--dest needs a path")?;
                }
                "--help" | "-h" => {
                    print_help();
                    std::process::exit(0);
                }
                other => {
                    return Err(format!("unknown argument `{other}`").into());
                }
            }
        }

        Ok(Self {
            source,
            harvest_source,
            destination,
        })
    }
}

fn print_help() {
    println!(
        "Sync the Autumn and Autumn Harvest guide docs into this site's vendored snapshot.\n\
\n\
Usage:\n\
  cargo run --bin sync_guide_docs -- \\\n\
    [--source <autumn-repo-or-docs-guide>] \\\n\
    [--harvest-source <autumn-harvest-repo-or-docs-getting-started>] \\\n\
    [--dest content/guide]\n\
\n\
Environment:\n\
  {SOURCE_ENV}=<autumn-repo-or-docs-guide>\n\
  {HARVEST_SOURCE_ENV}=<autumn-harvest-repo-or-docs-getting-started>\n\
\n\
Defaults:\n\
  source:         {DEFAULT_AUTUMN_REPO}\n\
  harvest-source: {DEFAULT_HARVEST_REPO}\n\
  dest:           {DEFAULT_DESTINATION}"
    );
}

fn guide_source_dir(source: &Path) -> Result<PathBuf, Box<dyn Error>> {
    let nested = source.join("docs").join("guide");
    if nested.is_dir() {
        return Ok(nested);
    }

    if source.join("getting-started.md").is_file() {
        return Ok(source.to_owned());
    }

    Err(format!(
        "{} is neither an Autumn repo root nor a docs/guide directory",
        source.display()
    )
    .into())
}

/// Collect every ATX heading in a guide body, outside fenced code blocks.
fn headings(markdown: &str) -> Vec<String> {
    let mut headings = Vec::new();
    let mut in_fence = false;

    for line in markdown.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        if let Some(rest) = line.strip_prefix('#') {
            let heading = rest.trim_start_matches('#');
            if heading.starts_with(' ') {
                headings.push(heading.trim().to_owned());
            }
        }
    }

    headings
}

/// GitHub's heading-anchor convention: lowercase, drop everything that is not a
/// word character, space, or hyphen, then spaces to hyphens. Underscores survive
/// and runs of punctuation collapse to nothing rather than to a separator, so
/// `#[secured("role")]` anchors as `securedrole`.
/// The tail that an issue or PR reference leaves in a heading anchor:
/// `-issue-1802`, `--pr-77` or a bare `-819` at the end.
static FRAGMENT_REFERENCE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"-+(?:(?:issues?|prs?)-\d+|\d{2,}$)").unwrap_or_else(|error| panic!("{error}"))
});

fn github_anchor(heading: &str) -> String {
    let mut anchor = String::with_capacity(heading.len());

    for char in heading.trim().chars().flat_map(char::to_lowercase) {
        if char.is_alphanumeric() || char == '_' || char == '-' {
            anchor.push(char);
        } else if char.is_whitespace() {
            anchor.push('-');
        }
    }

    anchor
}

/// Rewrite link fragments authored against GitHub's anchor convention onto the
/// IDs this site's renderer actually emits.
///
/// Upstream guides live on GitHub, so their in-page and cross-page fragments use
/// GitHub's convention — `#securedrole`, `#api_doc`, `#lock_version`. The
/// renderer slugifies headings differently (`secured-role`, `api-doc`), so those
/// links land on the right page but never jump to the heading.
///
/// A fragment is rewritten only when it fails to match any heading ID on the
/// target page *and* matches that page's GitHub anchor for a real heading. Every
/// other fragment — already correct, hand-written, or pointing at something we
/// cannot resolve — is left exactly as it was, so a fragment this cannot place
/// is never made worse.
fn normalize_fragments(
    markdown: &str,
    slug: &str,
    headings: &HashMap<&str, Vec<String>>,
) -> String {
    let mut output = String::with_capacity(markdown.len());
    let mut in_fence = false;

    for line in markdown.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
        }
        if in_fence {
            output.push_str(line);
            output.push('\n');
            continue;
        }

        let mut rest = line;
        while let Some(open) = rest.find("](") {
            let after = &rest[open + 2..];
            let Some(close) = after.find(')') else {
                break;
            };
            output.push_str(&rest[..open + 2]);
            output.push_str(&normalize_destination(&after[..close], slug, headings));
            output.push(')');
            rest = &after[close + 1..];
        }
        output.push_str(rest);
        output.push('\n');
    }

    output
}

fn normalize_destination(
    destination: &str,
    slug: &str,
    headings: &HashMap<&str, Vec<String>>,
) -> String {
    let Some((path, fragment)) = destination.split_once('#') else {
        return destination.to_owned();
    };
    if fragment.is_empty() {
        return destination.to_owned();
    }

    let Some(target) = fragment_target_slug(path, slug) else {
        return destination.to_owned();
    };
    let Some(target_headings) = headings.get(target.as_str()) else {
        return destination.to_owned();
    };

    // Already a real heading ID on the target page — nothing to do.
    if target_headings
        .iter()
        .any(|heading| slugify_heading(heading) == fragment)
    {
        return destination.to_owned();
    }

    // The sync strips issue references from headings, so a fragment written
    // against the old heading can end in `-issue-1802` or `-819`. Try it
    // again without that tail. Only a real heading on the target page counts.
    let without_references = FRAGMENT_REFERENCE.replace_all(fragment, "");
    target_headings
        .iter()
        .find(|heading| github_anchor(heading) == fragment)
        .or_else(|| {
            target_headings.iter().find(|heading| {
                slugify_heading(heading) == without_references
                    || github_anchor(heading) == without_references
            })
        })
        .map_or_else(
            || destination.to_owned(),
            |heading| format!("{path}#{}", slugify_heading(heading)),
        )
}

/// The vendored slug a link destination points at, for the link shapes that can
/// reach a guide page: an empty path (same page), a sibling or `docs/guide/`
/// Markdown path, or an already-resolved `/docs/{slug}` route.
fn fragment_target_slug(path: &str, slug: &str) -> Option<String> {
    if path.is_empty() {
        return Some(slug.to_owned());
    }
    if let Some(route) = path.strip_prefix("/docs/") {
        return (!route.is_empty() && !route.contains('/')).then(|| route.to_owned());
    }

    let relative = path.strip_prefix("./").unwrap_or(path);
    let relative = relative.strip_prefix("docs/guide/").unwrap_or(relative);
    let stem = relative.strip_suffix(".md")?;

    (!stem.is_empty() && !stem.contains('/')).then(|| stem.to_owned())
}

/// Close heading-level skips: an ATX heading is placed exactly one level below
/// the nearest preceding heading its author put at a shallower level, so a
/// `####` straight under a `##` becomes a `###`.
///
/// A skipped level breaks the document outline screen-reader users navigate by
/// — axe reports it as `heading-order`. Upstream authors these by hand, and a
/// fix made to the vendored copy is lost on the next sync (`harvest-signals`
/// regressed exactly that way), so it is enforced here instead. Levels are
/// tracked against the *authored* outline, so headings authored as siblings
/// stay siblings and deeper nesting under a raised heading moves up with it.
/// Only the level changes: heading IDs come from the heading text, so no
/// anchor moves.
///
/// Headings are found by the same CommonMark parser, with the same options,
/// that renders the site, not by scanning lines. This pass rewrites content,
/// and a `#` line inside a tilde fence, a longer backtick fence, or an
/// indented code block is code: line heuristics misread each of those in turn.
fn clamp_heading_levels(markdown: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_HEADING_ATTRIBUTES);
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);

    // (authored level, emitted level) for each open ancestor heading.
    let mut ancestors: Vec<(usize, usize)> = Vec::new();
    // (byte offset of the heading's `#` run, authored level, emitted level).
    let mut raises: Vec<(usize, usize, usize)> = Vec::new();

    for (event, range) in Parser::new_ext(markdown, options).into_offset_iter() {
        let Event::Start(Tag::Heading { level, .. }) = event else {
            continue;
        };
        let level = level as usize;
        while ancestors
            .last()
            .is_some_and(|(authored, _)| *authored >= level)
        {
            ancestors.pop();
        }
        let emitted = ancestors.last().map_or(level, |(_, parent)| parent + 1);
        ancestors.push((level, emitted));

        // Only an ATX heading carries its level as a `#` run to rewrite. A
        // setext heading is level 1 or 2 and so never needs raising.
        if emitted < level && markdown[range.start..].starts_with(&"#".repeat(level)) {
            raises.push((range.start, level, emitted));
        }
    }

    let mut output = String::with_capacity(markdown.len());
    let mut copied = 0;
    for (start, level, emitted) in raises {
        output.push_str(&markdown[copied..start]);
        output.push_str(&"#".repeat(emitted));
        copied = start + level;
    }
    output.push_str(&markdown[copied..]);
    output
}

/// Issue and PR numbers in the upstream docs tie a passage to its source
/// change. A reader of the public guides has no use for them, so the sync
/// removes them. It removes these shapes, outside code:
///
/// - a parenthetical that holds only references: `(issue #2269)`, `(#2283)`,
///   `(issues #342, #1792)`, `(see [#1190](…))`;
/// - a reference at the start or end of a parenthetical:
///   `(issue #684, and the rest)` becomes `(and the rest)`;
/// - a sentence that holds only references: `Issue #953.` or
///   `**Issue #958.**`;
/// - a metadata line such as `**Issue:** [#1190](…)`.
///
/// A reference inside running prose (`Before issue #1667 …`) stays. Its
/// sentence needs an edit upstream.
fn strip_issue_references(markdown: &str) -> String {
    static RULES: LazyLock<IssueReferenceRules> = LazyLock::new(IssueReferenceRules::new);
    let rules = &*RULES;

    let mut output = String::with_capacity(markdown.len());
    let mut in_fence = false;
    for line in markdown.lines() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
        } else if !in_fence {
            if rules.metadata_line.is_match(line) {
                continue;
            }
            let stripped = rules.strip_line(line);
            // A line that held only a reference sentence goes away.
            let rest = stripped.trim();
            if (rest.is_empty() || rest == ">") && line.trim() != rest {
                continue;
            }
            output.push_str(&stripped);
            output.push('\n');
            continue;
        }
        output.push_str(line);
        output.push('\n');
    }
    output
}

/// The patterns behind [`strip_issue_references`].
struct IssueReferenceRules {
    parenthetical: Regex,
    leading: Regex,
    trailing: Regex,
    sentence: Regex,
    bold_sentence: Regex,
    metadata_line: Regex,
}

impl IssueReferenceRules {
    fn new() -> Self {
        // One reference: a linked `[#123](…)` or a bare `#123`.
        let one = r"(?:\[#\d+\]\([^)]*\)|#\d{2,}\b)";
        // A list of references: `#342, #1792`, `#12 and #13`.
        let many = format!(r"{one}(?:\s*(?:,|and|/|&)\s*(?:{one}|\d{{2,}}\b))*");
        let lead_in = r"(?i:(?:(?:see|tracked in|tracking|follow-?up(?: to)?|from|per|part of)\s+)?(?:issues?|prs?|pull requests?)?\s*)";
        let build =
            |pattern: String| Regex::new(&pattern).unwrap_or_else(|error| panic!("{error}"));
        Self {
            parenthetical: build(format!(r"(^|[^\]\s])\s*\(\s*{lead_in}{many}\s*\)")),
            leading: build(format!(r"\(\s*{lead_in}{many}\s*[,;:]\s*")),
            trailing: build(format!(r"\s*(?:[,;]|\s[—–-])\s*{lead_in}{many}\s*\)")),
            sentence: build(format!(r"^(\s*(?:>\s*)?\*?)(?i:issues?)\s+{many}\.\s*")),
            bold_sentence: build(format!(
                r"^(\s*(?:>\s*)?)\*\*(?i:issues?)\s+{many}\.\*\*\s*"
            )),
            metadata_line: build(format!(
                r"^\s*(?:>\s*)?(?:[-*]\s+)?\*\*(?i:issues?|prs?):?\*\*:?\s*{many}\.?\s*$"
            )),
        }
    }

    /// Strip one line. Inline code spans stay as they are.
    fn strip_line(&self, line: &str) -> String {
        let mut output = String::with_capacity(line.len());
        for (index, segment) in line.split('`').enumerate() {
            if index > 0 {
                output.push('`');
            }
            if index % 2 == 1 {
                output.push_str(segment);
                continue;
            }
            let text = self.parenthetical.replace_all(segment, "$1");
            let text = self.leading.replace_all(&text, "(");
            let text = self.trailing.replace_all(&text, ")");
            let text = if index == 0 {
                let text = self.bold_sentence.replace(&text, "$1");
                self.sentence.replace(&text, "$1").into_owned()
            } else {
                text.into_owned()
            };
            output.push_str(&text);
        }
        output
    }
}

/// The site slug a guide file is served at: its file stem. Entries in
/// [`GUIDE_FILES`] may be nested under `docs/guide/` (`observability/`), and the
/// site's guide namespace is flat, so the directory prefix is dropped.
fn guide_slug(file_name: &str) -> &str {
    let stem = file_name.trim_end_matches(".md");
    stem.rsplit('/').next().unwrap_or(stem)
}

fn first_heading(markdown: &str) -> Option<String> {
    markdown
        .lines()
        .find_map(|line| line.strip_prefix("# ").map(|title| title.trim().to_owned()))
        .filter(|title| !title.is_empty())
}

fn first_description(markdown: &str) -> Option<String> {
    let mut seen_heading = false;
    let mut in_fence = false;
    let mut paragraph = String::new();

    for line in markdown.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("```") {
            in_fence = !in_fence;
            continue;
        }
        if in_fence {
            continue;
        }
        if trimmed.starts_with("# ") {
            seen_heading = true;
            continue;
        }
        if !seen_heading || trimmed == "---" {
            continue;
        }
        if trimmed.is_empty() {
            if !paragraph.is_empty() {
                break;
            }
            continue;
        }
        if trimmed.starts_with('#')
            || trimmed.starts_with('>')
            || trimmed.starts_with('-')
            || trimmed.starts_with('|')
        {
            continue;
        }

        if !paragraph.is_empty() {
            paragraph.push(' ');
        }
        paragraph.push_str(trimmed);
    }

    let description = strip_inline_markdown(&paragraph);
    if description.is_empty() {
        None
    } else {
        Some(description)
    }
}

fn strip_inline_markdown(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    let mut in_code = false;
    let mut chars = value.chars().peekable();

    while let Some(char) = chars.next() {
        match char {
            '`' => in_code = !in_code,
            '[' if !in_code => {
                while let Some(label_char) = chars.next() {
                    if label_char == ']' {
                        if chars.peek() == Some(&'(') {
                            for url_char in chars.by_ref() {
                                if url_char == ')' {
                                    break;
                                }
                            }
                        }
                        break;
                    }
                    output.push(label_char);
                }
            }
            '*' | '_' if !in_code => {}
            _ => output.push(char),
        }
    }

    output.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn toml_string(value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    format!("\"{escaped}\"")
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{
        HARVEST_DOCS_DIR, HARVEST_GUIDE_DIR, HARVEST_GUIDE_FILES, HARVEST_REFERENCE_FILES,
        clamp_heading_levels, clean_harvest_title, clean_reference_title, normalize_destination,
        rebase_nested_links, replace_first_heading, rewrite_harvest_destination,
    };

    #[test]
    fn harvest_links_resolve_to_site_pages_from_both_directories() {
        // A chapter links to a reference doc one level up.
        assert_eq!(
            rewrite_harvest_destination("../durable-routes.md#keys", HARVEST_GUIDE_DIR),
            "/docs/harvest-durable-routes#keys"
        );
        // A reference doc links to a chapter, a sibling and the guide index.
        assert_eq!(
            rewrite_harvest_destination("getting-started/03-durable-timers.md", HARVEST_DOCS_DIR),
            "/docs/harvest-durable-timers"
        );
        assert_eq!(
            rewrite_harvest_destination("./agent-mesh.md", HARVEST_DOCS_DIR),
            "/docs/harvest-agent-mesh"
        );
        assert_eq!(
            rewrite_harvest_destination("getting-started/README.md", HARVEST_DOCS_DIR),
            "/docs/autumn-harvest"
        );
        // A file the site does not vendor goes to the source repository.
        assert_eq!(
            rewrite_harvest_destination("architecture.md#locks", HARVEST_DOCS_DIR),
            "https://github.com/autumn-foundation/autumn-harvest/blob/trunk-dev/docs/architecture.md#locks"
        );
        assert_eq!(
            rewrite_harvest_destination("../autumn-harvest-agent/src", HARVEST_DOCS_DIR),
            "https://github.com/autumn-foundation/autumn-harvest/tree/trunk-dev/autumn-harvest-agent/src"
        );
        // Chapter links keep working.
        assert_eq!(
            rewrite_harvest_destination("04-signals.md", HARVEST_GUIDE_DIR),
            "/docs/harvest-signals"
        );
    }

    #[test]
    fn reference_definitions_are_rewritten_too() {
        let body = "[replay canary]: ./replay-verify.md \"Canary\"\n\
                    [up]: https://example.com/a.md\n\
                    ```\n[code]: ./replay-verify.md\n```\n";
        assert_eq!(
            super::rewrite_harvest_links(body, HARVEST_DOCS_DIR),
            "[replay canary]: /docs/harvest-replay-verify \"Canary\"\n\
             [up]: https://example.com/a.md\n\
             ```\n[code]: ./replay-verify.md\n```\n"
        );
    }

    #[test]
    fn issue_references_are_stripped_from_guides() {
        use super::strip_issue_references as strip;

        // Headings and parentheticals that hold only references.
        assert_eq!(
            strip("## 13. Durable peers (issue #2269)\n"),
            "## 13. Durable peers\n"
        );
        assert_eq!(strip("### Hard deletes (#2265)\n"), "### Hard deletes\n");
        assert_eq!(
            strip("Retry jitter (issues #342, #1792) is deterministic.\n"),
            "Retry jitter is deterministic.\n"
        );
        assert_eq!(
            strip("The flag (see [#1190](https://github.com/a/b/issues/1190)).\n"),
            "The flag.\n"
        );
        // A reference at the start or end of a longer parenthetical.
        assert_eq!(
            strip("Shards (issue #684, reconciled below) move.\n"),
            "Shards (reconciled below) move.\n"
        );
        assert_eq!(
            strip("Shards (reconciled; #1273) move.\n"),
            "Shards (reconciled) move.\n"
        );
        // A sentence or a metadata line that holds only references.
        assert_eq!(
            strip("Issue #953. Ships every record off-box.\n"),
            "Ships every record off-box.\n"
        );
        assert_eq!(
            strip("*Issue #843. Builds on retry.*\n"),
            "*Builds on retry.*\n"
        );
        assert_eq!(
            strip("> Issue #1982. A live signal.\n"),
            "> A live signal.\n"
        );
        assert_eq!(
            strip("a\n- **Issue:** [#1190](https://x/1190)\nb\n"),
            "a\nb\n"
        );
        assert_eq!(
            strip("**Issue #958.** An opt-in layout.\n"),
            "An opt-in layout.\n"
        );
        assert_eq!(
            strip("> Tip.\n> Issue [#1716](https://x/1716).\n"),
            "> Tip.\n"
        );
        assert_eq!(
            strip("Gauges (canonical SLI, issue #501) and tokens (opt-in — issue #942).\n"),
            "Gauges (canonical SLI) and tokens (opt-in).\n"
        );
        assert_eq!(
            strip("(`autumn schema`, tracking issue #1975)\n"),
            "(`autumn schema`)\n"
        );
    }

    #[test]
    fn fragments_find_headings_after_references_are_stripped() {
        let headings = HashMap::from([(
            "security",
            vec![
                "Scoped API tokens (built in, opt-in)".to_owned(),
                "Step-up vs session rotation".to_owned(),
                "403 vs 404".to_owned(),
            ],
        )]);
        let fix = |destination: &str| normalize_destination(destination, "security", &headings);

        assert_eq!(
            fix("/docs/security#scoped-api-tokens-built-in-opt-in--issue-942"),
            "/docs/security#scoped-api-tokens-built-in-opt-in"
        );
        assert_eq!(
            fix("#step-up-vs-session-rotation-819"),
            "#step-up-vs-session-rotation"
        );
        // A real heading ID stays, and an unknown fragment is not touched.
        assert_eq!(fix("#403-vs-404"), "#403-vs-404");
        assert_eq!(fix("#missing-issue-12"), "#missing-issue-12");
    }

    #[test]
    fn issue_reference_stripping_leaves_other_text_alone() {
        use super::strip_issue_references as strip;

        for kept in [
            // Code, inline and fenced.
            "Call `wait(#12)` here.\n",
            "```\nlet x = (#123);\n```\n",
            // Anchors, links and colors.
            "See [the decision](#403-vs-404).\n",
            "See [step two](#12).\n",
            "color: #0284c7;\n",
            // A reference in running prose needs an upstream edit.
            "Before issue #1667 the flag did nothing.\n",
            "Rust attributes such as #[derive(Debug)] stay.\n",
        ] {
            assert_eq!(strip(kept), kept);
        }
    }

    #[test]
    fn harvest_slugs_are_unique_and_prefixed() {
        let mut slugs: Vec<&str> = HARVEST_GUIDE_FILES
            .iter()
            .chain(HARVEST_REFERENCE_FILES)
            .map(|(_, slug, _)| *slug)
            .collect();
        assert!(slugs.iter().all(|slug| slug.starts_with("harvest-")));
        let count = slugs.len();
        slugs.sort_unstable();
        slugs.dedup();
        assert_eq!(slugs.len(), count, "two Harvest files share a slug");
    }

    #[test]
    fn reference_titles_drop_the_issue_suffix() {
        assert_eq!(
            clean_reference_title("Calendar-Aware Schedules (issue #337)"),
            "Calendar-Aware Schedules"
        );
        assert_eq!(
            clean_reference_title("Retry jitter (issues #342, #1792)"),
            "Retry jitter"
        );
        assert_eq!(clean_reference_title("Durable routes"), "Durable routes");
        assert_eq!(
            clean_reference_title("MCP Tools — expose `#[workflow]`s to AI agents (issue #597)"),
            "MCP Tools"
        );
        assert_eq!(
            clean_reference_title("`harvest-verify` — semantic determinism verification"),
            "harvest-verify"
        );
        assert_eq!(
            replace_first_heading(
                "# Fairness keys (issue #1976)\n\nBody\n# Two\n",
                "Fairness keys"
            ),
            "# Fairness keys\n\nBody\n# Two\n"
        );
    }

    #[test]
    fn nested_guide_links_are_rebased_onto_the_guide_root() {
        let body = "See [metrics](../metrics.md#counters), [guide](../../migrations/0.9.0.md), \
                    [peer](server-timing.md), [up](https://example.com/a.md) and [top](#top).\n";
        assert_eq!(
            rebase_nested_links(body, "observability/overload-signals.md"),
            "See [metrics](metrics.md#counters), [guide](../migrations/0.9.0.md), \
             [peer](observability/server-timing.md), [up](https://example.com/a.md) and [top](#top).\n"
        );
    }

    #[test]
    fn top_level_guide_links_are_unchanged() {
        let body = "[a](../x.md)\n";
        assert_eq!(rebase_nested_links(body, "metrics.md"), body);
    }

    #[test]
    fn numbered_chapter_headings_lose_their_chapter_label() {
        assert_eq!(
            clean_harvest_title("Chapter 1 — Project skeleton"),
            "Project skeleton"
        );
        assert_eq!(clean_harvest_title("Chapter 12: Webhooks"), "Webhooks");
    }

    #[test]
    fn the_fork_chapter_heading_loses_its_fork_label() {
        assert_eq!(
            clean_harvest_title("Fork — The first workflow on plain Axum"),
            "The first workflow on plain Axum"
        );
    }

    #[test]
    fn headings_that_merely_start_with_a_label_word_are_kept() {
        assert_eq!(
            clean_harvest_title("Forking workflows"),
            "Forking workflows"
        );
        assert_eq!(clean_harvest_title("Fork"), "Fork");
        assert_eq!(clean_harvest_title("Chapter 3"), "Chapter 3");
    }

    #[test]
    fn a_skipped_heading_level_is_raised_to_one_below_its_parent() {
        let markdown = "# Title\n\n## Section\n\n#### Skipped\n\n### Sibling\n";
        assert_eq!(
            clamp_heading_levels(markdown),
            "# Title\n\n## Section\n\n### Skipped\n\n### Sibling\n"
        );
    }

    #[test]
    fn headings_authored_as_siblings_stay_siblings_when_raised() {
        let markdown = "# T\n## A\n#### B\n#### C\n";
        assert_eq!(clamp_heading_levels(markdown), "# T\n## A\n### B\n### C\n");
    }

    #[test]
    fn nesting_under_a_raised_heading_moves_up_with_it() {
        let markdown = "# T\n## A\n#### B\n##### B1\n## D\n";
        assert_eq!(
            clamp_heading_levels(markdown),
            "# T\n## A\n### B\n#### B1\n## D\n"
        );
    }

    #[test]
    fn hashes_inside_tilde_and_long_backtick_fences_are_code() {
        // A tilde fence, and a four-backtick fence quoting a ``` line: neither
        // `####` is a heading, and the `#` shell comment must not reset the
        // outline so that the real `###` after it gets raised.
        let markdown = "# T\n## A\n~~~sh\n# install\n#### output\n~~~\n### B\n\
                        ````md\n```\n#### quoted\n```\n````\n### C\n";
        assert_eq!(clamp_heading_levels(markdown), markdown);
    }

    #[test]
    fn a_fence_closes_only_on_its_own_marker_and_length() {
        // `~~~` cannot close a backtick fence, nor ``` a ```` one, so the
        // `####` lines stay code; the heading after the real close is clamped.
        let markdown = "# T\n## A\n````\n~~~\n```\n#### code\n````\n#### D\n";
        assert_eq!(
            clamp_heading_levels(markdown),
            "# T\n## A\n````\n~~~\n```\n#### code\n````\n### D\n"
        );
    }

    #[test]
    fn an_unmatched_fence_line_in_indented_code_does_not_hide_later_headings() {
        // The four-space-indented block is code, so its lone ``` opens no
        // fence, and the `####` after it is still clamped.
        let markdown = "# T\n\n## A\n\n    ```\n    #### code\n\n#### B\n";
        assert_eq!(
            clamp_heading_levels(markdown),
            "# T\n\n## A\n\n    ```\n    #### code\n\n### B\n"
        );
    }

    #[test]
    fn headings_inside_containers_are_clamped_in_place() {
        // A heading in a blockquote is part of the outline; only its `#` run
        // changes, never the `> ` prefix in front of it.
        let markdown = "# T\n\n## A\n\n> #### Quoted\n";
        assert_eq!(
            clamp_heading_levels(markdown),
            "# T\n\n## A\n\n> ### Quoted\n"
        );
    }

    #[test]
    fn well_ordered_headings_and_fenced_hashes_are_untouched() {
        let markdown = "# T\n## A\n### B\n## C\n\n```sh\n#### not a heading\n```\n#hashtag\n";
        assert_eq!(clamp_heading_levels(markdown), markdown);
    }
}

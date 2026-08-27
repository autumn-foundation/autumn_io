use std::collections::HashMap;
use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use autumn_io::docs::slugify_heading;

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
        let raw = rewrite_org_urls(&raw);
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
        let body = normalize_fragments(&guide.body, &guide.slug, &headings);
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
        let raw = rewrite_org_urls(&raw);
        let raw = rewrite_harvest_links(&raw);

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

    Ok(guides)
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
/// heading, leaving a clean sidebar/title label (`Project skeleton`).
fn clean_harvest_title(heading: &str) -> String {
    let heading = heading.trim();
    if let Some(rest) = heading.strip_prefix("Chapter ") {
        let after_number = rest.trim_start_matches(|char: char| char.is_ascii_digit());
        let cleaned = after_number
            .trim_start()
            .trim_start_matches(['—', ':', '-'])
            .trim();
        if !cleaned.is_empty() {
            return cleaned.to_owned();
        }
    }
    heading.to_owned()
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
fn rewrite_harvest_links(markdown: &str) -> String {
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

        output.push_str(&rewrite_harvest_links_in_line(line));
        output.push('\n');
    }

    output
}

fn rewrite_harvest_links_in_line(line: &str) -> String {
    let mut output = String::with_capacity(line.len());
    let mut rest = line;

    while let Some(open) = rest.find("](") {
        let after = &rest[open + 2..];
        let Some(close) = after.find(')') else {
            break;
        };
        output.push_str(&rest[..open + 2]);
        output.push_str(&rewrite_harvest_destination(&after[..close]));
        output.push(')');
        rest = &after[close + 1..];
    }

    output.push_str(rest);
    output
}

fn rewrite_harvest_destination(destination: &str) -> String {
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
    let path = path.strip_prefix("./").unwrap_or(path);

    if let Some((_, slug, _)) = HARVEST_GUIDE_FILES
        .iter()
        .find(|(file_name, _, _)| *file_name == path)
    {
        return format!("/docs/{slug}{fragment}");
    }

    if path == "README.md" {
        return format!("/docs/{HARVEST_INTRO_SLUG}{fragment}");
    }

    let resolved = resolve_harvest_source_path(path);
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

/// Resolve a chapter-relative path (from `docs/getting-started/`) into a
/// repository-root path for an absolute `autumn-harvest` source link.
fn resolve_harvest_source_path(path: &str) -> String {
    let mut segments = vec!["docs", "getting-started"];
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

    match target_headings
        .iter()
        .find(|heading| github_anchor(heading) == fragment)
    {
        Some(heading) => format!("{path}#{}", slugify_heading(heading)),
        None => destination.to_owned(),
    }
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

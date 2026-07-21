use std::env;
use std::error::Error;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

const DEFAULT_AUTUMN_REPO: &str = "../autumn";
const DEFAULT_DESTINATION: &str = "content/guide";
const SOURCE_ENV: &str = "AUTUMN_REPO_DIR";

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
];

fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse(env::args_os().skip(1))?;
    let source = guide_source_dir(&args.source)?;

    fs::create_dir_all(&args.destination)?;
    for (file_name, order) in GUIDE_FILES {
        let source_file = source.join(file_name);
        let raw = fs::read_to_string(&source_file)
            .map_err(|error| format!("failed to read {}: {error}", source_file.display()))?;
        let title = first_heading(&raw).ok_or_else(|| {
            format!(
                "guide file {} must start with a level-one heading",
                source_file.display()
            )
        })?;
        let description = first_description(&raw).unwrap_or_else(|| title.clone());
        let slug = file_name.trim_end_matches(".md");

        let output = format!(
            "+++\ntitle = {}\ndescription = {}\norder = {order}\n+++\n\n{}",
            toml_string(&title),
            toml_string(&description),
            raw.trim_start()
        );
        fs::write(args.destination.join(format!("{slug}.md")), output)?;
    }

    Ok(())
}

struct Args {
    source: PathBuf,
    destination: PathBuf,
}

impl Args {
    fn parse(args: impl IntoIterator<Item = OsString>) -> Result<Self, Box<dyn Error>> {
        let mut source = env::var_os(SOURCE_ENV)
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(DEFAULT_AUTUMN_REPO));
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
            destination,
        })
    }
}

fn print_help() {
    println!(
        "Sync Autumn guide docs into this site's vendored snapshot.\n\
\n\
Usage:\n\
  cargo run --bin sync_guide_docs -- --source <autumn-repo-or-docs-guide> [--dest content/guide]\n\
\n\
Environment:\n\
  {SOURCE_ENV}=<autumn-repo-or-docs-guide>\n\
\n\
Defaults:\n\
  source: {DEFAULT_AUTUMN_REPO}\n\
  dest:   {DEFAULT_DESTINATION}"
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

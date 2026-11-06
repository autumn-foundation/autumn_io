//! Checks the crate versions that the guides tell readers to add.
//!
//! The guides come from the Autumn and Harvest repositories. A sync from a
//! new release can keep an old pin on a page that the release did not
//! touch, for example `autumn-harvest = "0.8"` after the move to 0.9. This
//! test fails on such a page. Fix the page at its source, then sync again.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use autumn_io::seo::{AUTUMN_VERSION, HARVEST_VERSION};

#[test]
fn guide_pins_name_the_release_of_the_site() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut problems = String::new();
    for file in markdown_files(&root.join("content")) {
        let text = std::fs::read_to_string(&file).expect("read a guide page");
        let rel = file
            .strip_prefix(root)
            .unwrap_or(&file)
            .display()
            .to_string();
        for (line, name, requirement) in pins(&text) {
            let Some(release) = release_of(&name) else {
                continue;
            };
            let (Some(pinned), Some(wanted)) = (series_of(&requirement), series_of(release)) else {
                continue;
            };
            if pinned != wanted {
                let _ = writeln!(
                    problems,
                    "{rel}:{line}: `{name} = \"{requirement}\"`, but the site documents {release}"
                );
            }
        }
    }
    assert!(
        problems.is_empty(),
        "stale crate pins in the guides:\n{problems}"
    );
}

#[test]
fn the_site_builds_on_the_release_that_it_documents() {
    let manifest = include_str!("../Cargo.toml");
    let line = manifest
        .lines()
        .find(|l| l.trim_start().starts_with("autumn-web"))
        .expect("Cargo.toml depends on autumn-web");
    let requirement = requirement_of(line.split_once('=').expect("a dependency line").1.trim())
        .expect("autumn-web has a version");
    assert_eq!(
        series_of(&requirement),
        series_of(AUTUMN_VERSION),
        "Cargo.toml uses autumn-web {requirement}, but seo::AUTUMN_VERSION is {AUTUMN_VERSION}"
    );
}

/// Gives the release that a crate follows. Plugins have their own versions.
fn release_of(name: &str) -> Option<&'static str> {
    if name.starts_with("autumn-harvest") {
        Some(HARVEST_VERSION)
    } else if name.starts_with("autumn-") && !name.starts_with("autumn-plugin-") {
        Some(AUTUMN_VERSION)
    } else {
        None
    }
}

/// Finds `name = "req"` and `name = { version = "req" ... }` pins.
fn pins(text: &str) -> Vec<(usize, String, String)> {
    let mut found = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let mut rest = line;
        while let Some(eq) = rest.find('=') {
            let name: String = rest[..eq]
                .trim_end()
                .chars()
                .rev()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            if !name.is_empty()
                && name != "version"
                && let Some(requirement) = requirement_of(rest[eq + 1..].trim_start())
            {
                found.push((index + 1, name, requirement));
            }
            rest = &rest[eq + 1..];
        }
    }
    found
}

/// Reads the version requirement at the start of a TOML value.
fn requirement_of(value: &str) -> Option<String> {
    if let Some(quoted) = value.strip_prefix('"') {
        return quoted.split('"').next().map(str::to_owned);
    }
    let table = value.strip_prefix('{')?.split('}').next()?;
    let after = table[table.find("version")? + "version".len()..].trim_start();
    let after = after.strip_prefix('=')?.trim_start().strip_prefix('"')?;
    after.split('"').next().map(str::to_owned)
}

/// Gives the semver series: `0.9` for `0.9.2`, `1` for `1.4`.
fn series_of(requirement: &str) -> Option<String> {
    let start = requirement.find(|c: char| c.is_ascii_digit())?;
    let version: String = requirement[start..]
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let mut parts = version.split('.').filter(|p| !p.is_empty());
    let major: u64 = parts.next()?.parse().ok()?;
    if major > 0 {
        return Some(major.to_string());
    }
    let minor: u64 = parts.next().map_or(Ok(0), str::parse).ok()?;
    if minor > 0 {
        return Some(format!("0.{minor}"));
    }
    let patch: u64 = parts.next().map_or(Ok(0), str::parse).ok()?;
    Some(format!("0.0.{patch}"))
}

fn markdown_files(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)
            .expect("read a content dir")
            .flatten()
        {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "md") {
                found.push(path);
            }
        }
    }
    found.sort();
    found
}

#[test]
fn a_stale_pin_is_found() {
    let text = "```toml\nautumn-harvest = { version = \"0.8\", features = [\"x\"] }\n```\n";
    assert_eq!(
        pins(text),
        vec![(2, "autumn-harvest".to_owned(), "0.8".to_owned())]
    );
    assert_eq!(series_of("0.8").as_deref(), Some("0.8"));
    assert_ne!(series_of("0.8"), series_of(HARVEST_VERSION));
    assert_eq!(release_of("autumn-plugin-motion"), None);
    assert_eq!(release_of("autumn-storage-s3"), Some(AUTUMN_VERSION));
}

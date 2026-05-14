use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use autumn_web::static_gen::{ManifestEntry, StaticManifest};

use crate::docs::DocRegistry;
use crate::{seo, site};

const STATIC_DIR: &str = "static";
const AUTUMN_WEB_VERSION: &str = "0.4.0";
const EXPORT_MARKER_FILE: &str = ".autumn-io-static-export";

/// Filesystem settings for exporting the Autumn website as static assets.
#[derive(Clone, Debug)]
pub struct ExportConfig {
    output_dir: PathBuf,
    static_dir: PathBuf,
}

impl ExportConfig {
    /// Create an export config that writes generated files to `output_dir`.
    pub fn new(output_dir: impl Into<PathBuf>) -> Self {
        Self {
            output_dir: output_dir.into(),
            static_dir: PathBuf::from(STATIC_DIR),
        }
    }

    /// Override the static asset source directory.
    #[must_use]
    pub fn with_static_dir(mut self, static_dir: impl Into<PathBuf>) -> Self {
        self.static_dir = static_dir.into();
        self
    }
}

/// Counts emitted during a static site export.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExportSummary {
    pub html_pages: usize,
    pub static_assets: usize,
    pub routes: usize,
}

/// Errors that can occur while exporting the static site.
#[derive(Debug)]
pub enum ExportError {
    UnsafeOutputDir(PathBuf),
    UnsafeOutputPath {
        output_dir: PathBuf,
        path: PathBuf,
    },
    Io {
        path: PathBuf,
        source: io::Error,
    },
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
}

impl Display for ExportError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsafeOutputDir(path) => {
                write!(
                    f,
                    "refusing to export into unsafe output path `{}`",
                    path.display()
                )
            }
            Self::UnsafeOutputPath { output_dir, path } => {
                write!(
                    f,
                    "refusing to write `{}` outside export root `{}`",
                    path.display(),
                    output_dir.display()
                )
            }
            Self::Io { path, source } => {
                write!(f, "filesystem error at `{}`: {source}", path.display())
            }
            Self::Json { path, source } => {
                write!(f, "failed to write JSON at `{}`: {source}", path.display())
            }
        }
    }
}

impl Error for ExportError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::UnsafeOutputDir(_) | Self::UnsafeOutputPath { .. } => None,
            Self::Io { source, .. } => Some(source),
            Self::Json { source, .. } => Some(source),
        }
    }
}

/// Render the bundled docs and site chrome into a static `dist/` tree.
pub fn export_site(
    registry: &DocRegistry,
    config: &ExportConfig,
) -> Result<ExportSummary, ExportError> {
    let output_dir = reset_output_dir(&config.output_dir, &config.static_dir)?;

    let mut routes = HashMap::new();
    write_html(
        &output_dir,
        Path::new("index.html"),
        site::render_home_page(registry).into_string(),
    )?;
    routes.insert(
        "/".to_owned(),
        ManifestEntry {
            file: "index.html".to_owned(),
            revalidate: None,
        },
    );

    for page in registry.pages() {
        let file = format!("docs/{}/index.html", page.slug);
        write_html(
            &output_dir,
            &PathBuf::from("docs").join(&page.slug).join("index.html"),
            site::render_docs_page(registry, page).into_string(),
        )?;
        routes.insert(
            seo::docs_path(&page.slug),
            ManifestEntry {
                file,
                revalidate: None,
            },
        );
    }

    write_text(&output_dir, Path::new("robots.txt"), seo::robots_txt())?;
    routes.insert(
        "/robots.txt".to_owned(),
        ManifestEntry {
            file: "robots.txt".to_owned(),
            revalidate: None,
        },
    );

    write_text(
        &output_dir,
        Path::new("sitemap.xml"),
        seo::sitemap_xml(registry),
    )?;
    routes.insert(
        "/sitemap.xml".to_owned(),
        ManifestEntry {
            file: "sitemap.xml".to_owned(),
            revalidate: None,
        },
    );

    let static_assets = copy_static_assets(&config.static_dir, &output_dir, Path::new(STATIC_DIR))?;
    write_manifest(&output_dir, Path::new("manifest.json"), routes.clone())?;

    Ok(ExportSummary {
        html_pages: registry.pages().len() + 1,
        static_assets,
        routes: routes.len(),
    })
}

fn reset_output_dir(output_dir: &Path, static_dir: &Path) -> Result<PathBuf, ExportError> {
    let normalized_output = normalized_absolute_path(output_dir)?;
    let normalized_static = normalized_absolute_path(static_dir)?;

    if is_unsafe_output_dir(output_dir, &normalized_output, &normalized_static) {
        return Err(ExportError::UnsafeOutputDir(output_dir.to_path_buf()));
    }

    if normalized_output.exists() {
        if !can_replace_output_dir(&normalized_output)? {
            return Err(ExportError::UnsafeOutputDir(output_dir.to_path_buf()));
        }
        fs::remove_dir_all(&normalized_output).map_err(|source| ExportError::Io {
            path: normalized_output.clone(),
            source,
        })?;
    }
    create_dir(&normalized_output)?;
    write_export_marker(&normalized_output)?;
    canonicalize_path(&normalized_output)
}

fn is_unsafe_output_dir(
    output_dir: &Path,
    normalized_output: &Path,
    normalized_static: &Path,
) -> bool {
    output_dir.as_os_str().is_empty()
        || output_dir.file_name().is_none()
        || output_dir
            .components()
            .any(|component| matches!(component, Component::ParentDir))
        || normalized_output == normalized_static
        || normalized_output.starts_with(normalized_static)
}

fn can_replace_output_dir(path: &Path) -> Result<bool, ExportError> {
    if !path.is_dir() {
        return Ok(false);
    }
    if path.file_name() == Some(std::ffi::OsStr::new("dist"))
        || path.join(EXPORT_MARKER_FILE).is_file()
    {
        return Ok(true);
    }

    let mut entries = fs::read_dir(path).map_err(|source| ExportError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    entries
        .next()
        .transpose()
        .map(|entry| entry.is_none())
        .map_err(|source| ExportError::Io {
            path: path.to_path_buf(),
            source,
        })
}

fn write_export_marker(output_dir: &Path) -> Result<(), ExportError> {
    let path = output_dir.join(EXPORT_MARKER_FILE);
    fs::write(&path, "Generated by autumn_io static export.\n")
        .map_err(|source| ExportError::Io { path, source })
}

fn write_html(output_dir: &Path, path: &Path, html: String) -> Result<(), ExportError> {
    write_text(output_dir, path, html)
}

fn write_text(output_dir: &Path, path: &Path, contents: String) -> Result<(), ExportError> {
    let path = output_path_for_write(output_dir, path)?;

    fs::write(&path, contents).map_err(|source| ExportError::Io { path, source })
}

fn create_dir(path: &Path) -> Result<(), ExportError> {
    fs::create_dir_all(path).map_err(|source| ExportError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn copy_static_assets(
    source: &Path,
    output_dir: &Path,
    destination: &Path,
) -> Result<usize, ExportError> {
    create_dir_under_output(output_dir, destination)?;
    let mut copied = 0;

    for entry in fs::read_dir(source).map_err(|source_error| ExportError::Io {
        path: source.to_path_buf(),
        source: source_error,
    })? {
        let entry = entry.map_err(|source_error| ExportError::Io {
            path: source.to_path_buf(),
            source: source_error,
        })?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let file_type = entry.file_type().map_err(|source_error| ExportError::Io {
            path: source_path.clone(),
            source: source_error,
        })?;

        if file_type.is_dir() {
            copied += copy_static_assets(&source_path, output_dir, &destination_path)?;
        } else if file_type.is_file() {
            let destination_path = output_path_for_write(output_dir, &destination_path)?;
            fs::copy(&source_path, &destination_path).map_err(|source_error| ExportError::Io {
                path: destination_path,
                source: source_error,
            })?;
            copied += 1;
        }
    }

    Ok(copied)
}

fn write_manifest(
    output_dir: &Path,
    path: &Path,
    routes: HashMap<String, ManifestEntry>,
) -> Result<(), ExportError> {
    let manifest = StaticManifest {
        generated_at: timestamp_now(),
        autumn_version: AUTUMN_WEB_VERSION.to_owned(),
        routes,
    };
    let json = serde_json::to_string_pretty(&manifest).map_err(|source| ExportError::Json {
        path: path.to_path_buf(),
        source,
    })?;
    write_text(output_dir, path, json)
}

fn output_path_for_write(output_dir: &Path, path: &Path) -> Result<PathBuf, ExportError> {
    let relative = safe_relative_path(path).ok_or_else(|| ExportError::UnsafeOutputPath {
        output_dir: output_dir.to_path_buf(),
        path: path.to_path_buf(),
    })?;
    let parent = relative.parent().unwrap_or_else(|| Path::new(""));
    let parent = create_dir_under_output(output_dir, parent)?;
    let Some(file_name) = relative.file_name() else {
        return Err(ExportError::UnsafeOutputPath {
            output_dir: output_dir.to_path_buf(),
            path: path.to_path_buf(),
        });
    };
    let path = parent.join(file_name);
    ensure_under_output(output_dir, &path)?;
    Ok(path)
}

fn create_dir_under_output(output_dir: &Path, path: &Path) -> Result<PathBuf, ExportError> {
    let relative = safe_relative_path(path).ok_or_else(|| ExportError::UnsafeOutputPath {
        output_dir: output_dir.to_path_buf(),
        path: path.to_path_buf(),
    })?;
    let directory = output_dir.join(relative);
    create_dir(&directory)?;
    let directory = canonicalize_path(&directory)?;
    ensure_under_output(output_dir, &directory)?;
    Ok(directory)
}

fn safe_relative_path(path: &Path) -> Option<PathBuf> {
    if path.as_os_str().is_empty() {
        return Some(PathBuf::new());
    }

    let mut safe = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Normal(segment) => safe.push(segment),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }

    if safe.as_os_str().is_empty() {
        None
    } else {
        Some(safe)
    }
}

fn ensure_under_output(output_dir: &Path, path: &Path) -> Result<(), ExportError> {
    if path.starts_with(output_dir) {
        Ok(())
    } else {
        Err(ExportError::UnsafeOutputPath {
            output_dir: output_dir.to_path_buf(),
            path: path.to_path_buf(),
        })
    }
}

fn normalized_absolute_path(path: &Path) -> Result<PathBuf, ExportError> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        current_dir()?.join(path)
    };

    normalize_path(&absolute).ok_or_else(|| ExportError::UnsafeOutputDir(path.to_path_buf()))
}

fn normalize_path(path: &Path) -> Option<PathBuf> {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::Normal(segment) => normalized.push(segment),
            Component::ParentDir => {
                if !normalized.pop() {
                    return None;
                }
            }
        }
    }

    Some(normalized)
}

fn canonicalize_path(path: &Path) -> Result<PathBuf, ExportError> {
    path.canonicalize().map_err(|source| ExportError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn current_dir() -> Result<PathBuf, ExportError> {
    std::env::current_dir().map_err(|source| ExportError::Io {
        path: PathBuf::from("."),
        source,
    })
}

fn timestamp_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{secs}")
}

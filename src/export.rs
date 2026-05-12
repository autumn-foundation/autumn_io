use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use autumn_web::static_gen::{ManifestEntry, StaticManifest};

use crate::docs::DocRegistry;
use crate::{seo, site};

const STATIC_DIR: &str = "static";
const AUTUMN_WEB_VERSION: &str = "0.3.0";

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
            Self::UnsafeOutputDir(_) => None,
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
    reset_output_dir(&config.output_dir)?;

    let mut routes = HashMap::new();
    write_html(
        &config.output_dir.join("index.html"),
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
            &config.output_dir.join(&file),
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

    write_text(&config.output_dir.join("robots.txt"), seo::robots_txt())?;
    routes.insert(
        "/robots.txt".to_owned(),
        ManifestEntry {
            file: "robots.txt".to_owned(),
            revalidate: None,
        },
    );

    write_text(
        &config.output_dir.join("sitemap.xml"),
        seo::sitemap_xml(registry),
    )?;
    routes.insert(
        "/sitemap.xml".to_owned(),
        ManifestEntry {
            file: "sitemap.xml".to_owned(),
            revalidate: None,
        },
    );

    let static_assets =
        copy_static_assets(&config.static_dir, &config.output_dir.join(STATIC_DIR))?;
    write_manifest(&config.output_dir.join("manifest.json"), routes.clone())?;

    Ok(ExportSummary {
        html_pages: registry.pages().len() + 1,
        static_assets,
        routes: routes.len(),
    })
}

fn reset_output_dir(output_dir: &Path) -> Result<(), ExportError> {
    if is_unsafe_output_dir(output_dir) {
        return Err(ExportError::UnsafeOutputDir(output_dir.to_path_buf()));
    }

    if output_dir.exists() {
        fs::remove_dir_all(output_dir).map_err(|source| ExportError::Io {
            path: output_dir.to_path_buf(),
            source,
        })?;
    }
    create_dir(output_dir)
}

fn is_unsafe_output_dir(output_dir: &Path) -> bool {
    output_dir.as_os_str().is_empty()
        || output_dir == Path::new(".")
        || output_dir == Path::new("..")
        || output_dir.file_name().is_none()
}

fn write_html(path: &Path, html: String) -> Result<(), ExportError> {
    write_text(path, html)
}

fn write_text(path: &Path, contents: String) -> Result<(), ExportError> {
    if let Some(parent) = path.parent() {
        create_dir(parent)?;
    }

    fs::write(path, contents).map_err(|source| ExportError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn create_dir(path: &Path) -> Result<(), ExportError> {
    fs::create_dir_all(path).map_err(|source| ExportError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn copy_static_assets(source: &Path, destination: &Path) -> Result<usize, ExportError> {
    create_dir(destination)?;
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
            copied += copy_static_assets(&source_path, &destination_path)?;
        } else if file_type.is_file() {
            if let Some(parent) = destination_path.parent() {
                create_dir(parent)?;
            }
            fs::copy(&source_path, &destination_path).map_err(|source_error| ExportError::Io {
                path: source_path,
                source: source_error,
            })?;
            copied += 1;
        }
    }

    Ok(copied)
}

fn write_manifest(path: &Path, routes: HashMap<String, ManifestEntry>) -> Result<(), ExportError> {
    let manifest = StaticManifest {
        generated_at: timestamp_now(),
        autumn_version: AUTUMN_WEB_VERSION.to_owned(),
        routes,
    };
    let json = serde_json::to_string_pretty(&manifest).map_err(|source| ExportError::Json {
        path: path.to_path_buf(),
        source,
    })?;
    write_text(path, json)
}

fn timestamp_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("{secs}")
}

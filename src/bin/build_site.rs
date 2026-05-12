use std::error::Error;

use autumn_io::export::{ExportConfig, export_site};

fn main() -> Result<(), Box<dyn Error>> {
    let registry =
        autumn_io::site_docs().map_err(|error| std::io::Error::other(error.to_string()))?;
    let summary = export_site(registry, &ExportConfig::new("dist"))?;

    println!(
        "Exported {} HTML pages, {} static assets, and {} manifest routes to dist/",
        summary.html_pages, summary.static_assets, summary.routes
    );

    Ok(())
}

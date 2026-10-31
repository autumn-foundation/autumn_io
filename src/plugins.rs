//! The plugin catalog behind the `/plugins` page.
//!
//! Each entry is a crate on crates.io. The page links to the crate, its API
//! docs on docs.rs and its source. The list holds no versions: a plugin
//! release and a site deploy happen at different times, and a version here
//! goes stale first.
//!
//! Add a plugin only after its first release on crates.io. A test checks the
//! shape of each entry, but it cannot check that the crate exists.

/// The path of the plugin catalog page.
pub const PLUGINS_PATH: &str = "/plugins";

/// The short text that introduces the catalog, on the page and on the home
/// page.
pub const PLUGINS_LEDE: &str = "Plugins add a database, a frontend runtime, a message \
     broker or a platform integration to an Autumn app. You install each one with \
     one call on the app builder.";

/// A group of plugins on the catalog page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginCategory {
    /// Islands, behaviors and rich widgets for Maud and htmx pages.
    Frontend,
    /// Animation, charts and canvas rendering.
    Graphics,
    /// Databases and query engines.
    Data,
    /// API servers and message brokers.
    Messaging,
    /// Chat platforms, commerce, analytics and third-party auth.
    Integrations,
    /// LLM agents and durable workflows.
    Agents,
    /// Telemetry, proxies and deployment targets.
    Operations,
    /// Plugins that ship from the Autumn workspace.
    Workspace,
}

impl PluginCategory {
    /// Every category, in page order.
    pub const ALL: [Self; 8] = [
        Self::Frontend,
        Self::Graphics,
        Self::Data,
        Self::Messaging,
        Self::Integrations,
        Self::Agents,
        Self::Operations,
        Self::Workspace,
    ];

    /// The section heading.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Frontend => "Frontend and UI",
            Self::Graphics => "Graphics and animation",
            Self::Data => "Data and storage",
            Self::Messaging => "APIs and messaging",
            Self::Integrations => "Integrations",
            Self::Agents => "Agents and workflows",
            Self::Operations => "Operations and deployment",
            Self::Workspace => "From the Autumn workspace",
        }
    }

    /// The fragment id of the section.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            Self::Frontend => "frontend",
            Self::Graphics => "graphics",
            Self::Data => "data",
            Self::Messaging => "messaging",
            Self::Integrations => "integrations",
            Self::Agents => "agents",
            Self::Operations => "operations",
            Self::Workspace => "workspace",
        }
    }
}

/// One published plugin crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plugin {
    /// The crate name on crates.io.
    pub crate_name: &'static str,
    /// One sentence on what the plugin gives an app.
    pub summary: &'static str,
    /// The source repository.
    pub repository: &'static str,
    /// The section that lists the plugin.
    pub category: PluginCategory,
}

impl Plugin {
    /// The crate page on crates.io.
    #[must_use]
    pub fn crates_io_url(&self) -> String {
        format!("https://crates.io/crates/{}", self.crate_name)
    }

    /// The API docs on docs.rs.
    #[must_use]
    pub fn docs_rs_url(&self) -> String {
        format!("https://docs.rs/{}", self.crate_name)
    }
}

/// The plugins in `category`, in catalog order.
pub fn plugins_in(category: PluginCategory) -> impl Iterator<Item = &'static Plugin> {
    PLUGINS
        .iter()
        .filter(move |plugin| plugin.category == category)
}

const fn plugin(
    crate_name: &'static str,
    repository: &'static str,
    category: PluginCategory,
    summary: &'static str,
) -> Plugin {
    Plugin {
        crate_name,
        summary,
        repository,
        category,
    }
}

macro_rules! repo {
    ($name:literal) => {
        concat!("https://github.com/autumn-foundation/", $name)
    };
}

use PluginCategory::{
    Agents, Data, Frontend, Graphics, Integrations, Messaging, Operations, Workspace,
};

/// Every published plugin, grouped by category and sorted by name in each
/// group.
pub const PLUGINS: &[Plugin] = &[
    // Frontend and UI.
    plugin(
        "autumn-plugin-leptos",
        repo!("autumn-plugin-leptos"),
        Frontend,
        "Leptos (wasm) islands in Maud and htmx pages.",
    ),
    plugin(
        "autumn-plugin-markstream",
        repo!("autumn-plugin-markstream"),
        Frontend,
        "Streaming Markdown over server-sent events, for LLM output and live logs.",
    ),
    plugin(
        "autumn-plugin-react",
        repo!("autumn-plugin-react"),
        Frontend,
        "React islands in Maud and htmx pages, with props updates over HX-Trigger.",
    ),
    plugin(
        "autumn-plugin-svelte",
        repo!("autumn-plugin-svelte"),
        Frontend,
        "Svelte 5 islands in Maud and htmx pages.",
    ),
    plugin(
        "autumn-plugin-topcoat",
        repo!("autumn-plugin-topcoat"),
        Frontend,
        "Autumn serves the backend and Topcoat renders the frontend, on typed routes.",
    ),
    plugin(
        "autumn-plugin-univer",
        repo!("autumn-plugin-univer"),
        Frontend,
        "Univer spreadsheets with typed workbook data and a save round trip.",
    ),
    plugin(
        "autumn-plugin-vanilla",
        repo!("autumn-plugin-vanilla"),
        Frontend,
        "Declarative vanilla JS behaviors, with no npm and no bundler.",
    ),
    // Graphics and animation.
    plugin(
        "autumn-plugin-d3",
        repo!("autumn-plugin-d3"),
        Graphics,
        "D3.js charts from a typed Rust builder, with an accessible data table.",
    ),
    plugin(
        "autumn-plugin-gsap",
        repo!("autumn-plugin-gsap"),
        Graphics,
        "GSAP tweens and timelines with typed builders, ScrollTrigger and SplitText.",
    ),
    plugin(
        "autumn-plugin-motion",
        "https://github.com/madmax983/autumn-plugin-motion",
        Graphics,
        "Motion (motion.dev) animations from data-motion attributes. This site uses it.",
    ),
    plugin(
        "autumn-plugin-pixijs",
        repo!("autumn-plugin-pixijs"),
        Graphics,
        "PixiJS 2D canvas stages from a typed Rust builder.",
    ),
    plugin(
        "autumn-plugin-three",
        repo!("autumn-plugin-three"),
        Graphics,
        "Three.js 3D scenes from a typed Rust builder.",
    ),
    // Data and storage.
    plugin(
        "autumn-plugin-aws-athena",
        repo!("autumn-plugin-aws-athena"),
        Data,
        "Amazon Athena SQL queries from handlers, with bound parameters and deadlines.",
    ),
    plugin(
        "autumn-plugin-clickhouse",
        repo!("autumn-plugin-clickhouse"),
        Data,
        "A ClickHouse OLAP client with batch inserts, typed queries and migrations.",
    ),
    plugin(
        "autumn-plugin-duckdb",
        repo!("autumn-plugin-duckdb"),
        Data,
        "DuckDB SQL queries from handlers, on blocking threads with deadlines.",
    ),
    plugin(
        "autumn-plugin-rocksdb",
        repo!("autumn-plugin-rocksdb"),
        Data,
        "A RocksDB key-value store that can also be the app cache and session store.",
    ),
    plugin(
        "autumn-plugin-surrealdb",
        repo!("autumn-plugin-surrealdb"),
        Data,
        "A SurrealDB client for documents, key-value data and graph queries.",
    ),
    plugin(
        "autumn-plugin-turso",
        repo!("autumn-plugin-turso"),
        Data,
        "An embedded, SQLite-compatible Turso database with migrations and sync.",
    ),
    // APIs and messaging.
    plugin(
        "autumn-plugin-aws-sqs",
        repo!("autumn-plugin-aws-sqs"),
        Messaging,
        "Amazon SQS as a job transport, with queue consumers and a producer.",
    ),
    plugin(
        "autumn-plugin-graphql",
        repo!("autumn-graphql-plugin"),
        Messaging,
        "Mount an async-graphql schema with query limits, persisted queries and subscriptions.",
    ),
    plugin(
        "autumn-plugin-grpc",
        repo!("autumn-plugin-grpc"),
        Messaging,
        "Serve tonic services with health, reflection, metrics and graceful drain.",
    ),
    plugin(
        "autumn-plugin-kafka",
        repo!("autumn-plugin-kafka"),
        Messaging,
        "A Kafka producer and consumer for Autumn apps.",
    ),
    // Integrations.
    plugin(
        "autumn-plugin-discord",
        repo!("autumn-plugin-discord"),
        Integrations,
        "A Discord bot framework: slash commands as routes, with signed interactions.",
    ),
    plugin(
        "autumn-plugin-meta-pixel",
        repo!("autumn-plugin-meta-pixel"),
        Integrations,
        "A consent-gated, CSP-safe Meta Pixel loader.",
    ),
    plugin(
        "autumn-plugin-san-mateo",
        repo!("san-mateo"),
        Integrations,
        "OAuth and API key connections with token refresh and an auth proxy.",
    ),
    plugin(
        "autumn-plugin-shopify",
        repo!("autumn-plugin-shopify"),
        Integrations,
        "Shopify apps: signed webhooks by topic and OAuth install with expiring tokens.",
    ),
    plugin(
        "autumn-plugin-slack",
        repo!("autumn-plugin-slack"),
        Integrations,
        "Verified Slack events, slash commands and interactivity, with a Web API client.",
    ),
    plugin(
        "autumn-plugin-teams",
        repo!("autumn-plugin-teams"),
        Integrations,
        "Verified Microsoft Teams bot activities with typed handlers.",
    ),
    // Agents and workflows.
    plugin(
        "autumn-harvest-plugin",
        repo!("autumn-harvest"),
        Agents,
        "Runs Autumn Harvest durable workflows inside an Autumn app.",
    ),
    plugin(
        "autumn-plugin-agent",
        repo!("autumn-plugin-agent"),
        Agents,
        "A provider-agnostic LLM agent harness with tools, sessions and memory.",
    ),
    // Operations and deployment.
    plugin(
        "autumn-plugin-aws-lambda",
        repo!("autumn-plugin-aws-lambda"),
        Operations,
        "Run an Autumn app on AWS Lambda.",
    ),
    plugin(
        "autumn-plugin-kubernetes",
        repo!("autumn-plugin-kubernetes"),
        Operations,
        "Pod identity, Lease leader election, live ConfigMaps and pod events.",
    ),
    plugin(
        "autumn-plugin-otel",
        repo!("autumn-plugin-otel"),
        Operations,
        "OpenTelemetry traces, metrics and logs over OTLP.",
    ),
    plugin(
        "autumn-plugin-pingora",
        repo!("autumn-plugin-pingora"),
        Operations,
        "A Pingora reverse proxy with routes, load balancing and health checks.",
    ),
    // From the Autumn workspace.
    plugin(
        "autumn-admin-plugin",
        repo!("autumn"),
        Workspace,
        "An admin panel for your models.",
    ),
    plugin(
        "autumn-billing",
        repo!("autumn"),
        Workspace,
        "Stripe subscription billing: checkout, portal, webhook mirror and dunning.",
    ),
    plugin(
        "autumn-cache-redis",
        repo!("autumn"),
        Workspace,
        "A shared Redis cache backend.",
    ),
    plugin(
        "autumn-media-plugin",
        repo!("autumn"),
        Workspace,
        "Live-streaming media: broadcasts and rooms.",
    ),
    plugin(
        "autumn-search",
        repo!("autumn"),
        Workspace,
        "Keyword and vector search: mark a model searchable and query it.",
    ),
    plugin(
        "autumn-storage-s3",
        repo!("autumn"),
        Workspace,
        "S3-compatible blob storage.",
    ),
];

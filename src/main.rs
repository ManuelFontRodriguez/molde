use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "molde", version = env!("MOLDE_VERSION"), about)]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Render the site into a new immutable build dir and atomically swap `current`.
    Build {
        /// Site folder: routes.yaml, data.json, templates/ (and optional site.yaml, static/, translations/).
        #[arg(long, default_value = "site")]
        site: PathBuf,
        /// Output folder: builds go to <OUT>/builds/<id>/, the live one is <OUT>/current.
        #[arg(long, default_value = "out")]
        out: PathBuf,
    },
    /// Build + all conformance layers -> ONE structured JSON report. Publishes only on pass.
    Check {
        /// Site folder: routes.yaml, data.json, templates/ (and optional site.yaml, static/, translations/).
        #[arg(long, default_value = "site")]
        site: PathBuf,
        /// Output folder: builds go to <OUT>/builds/<id>/, the live one is <OUT>/current.
        #[arg(long, default_value = "out")]
        out: PathBuf,
    },
    /// Serve out/current on localhost (dev loop for humans).
    Serve {
        /// Output folder: builds go to <OUT>/builds/<id>/, the live one is <OUT>/current.
        #[arg(long, default_value = "out")]
        out: PathBuf,
        /// Port on 127.0.0.1.
        #[arg(long, default_value_t = 8080)]
        port: u16,
        /// Watch the site dir and rerun the verified check pipeline on change.
        /// A failing check never publishes — the browser keeps the last good build.
        #[arg(long)]
        watch: bool,
        /// With --watch: auto-refresh open browser tabs when a new build publishes
        /// (serve-time snippet only; build artifacts stay byte-deterministic).
        #[arg(long, requires = "watch")]
        reload: bool,
        /// Site dir to watch/rebuild (default "site"; only meaningful with --watch,
        /// so requiring it makes a forgotten --watch loud instead of silent).
        #[arg(long, requires = "watch")]
        site: Option<PathBuf>,
    },
    /// Expose the site to agents over MCP on stdio: check, list_routes, explain_route,
    /// render_page, diff_build.
    Mcp {
        /// Site folder: routes.yaml, data.json, templates/ (and optional site.yaml, static/, translations/).
        #[arg(long, default_value = "site")]
        site: PathBuf,
        /// Output folder: builds go to <OUT>/builds/<id>/, the live one is <OUT>/current.
        #[arg(long, default_value = "out")]
        out: PathBuf,
    },
    /// Diff two immutable builds (default: the two most recent). JSON to stdout.
    Diff {
        /// Output folder: builds go to <OUT>/builds/<id>/, the live one is <OUT>/current.
        #[arg(long, default_value = "out")]
        out: PathBuf,
        /// Older build id, a folder name under <OUT>/builds (default: the one before --to).
        #[arg(long)]
        from: Option<String>,
        /// Newer build id (default: the newest build).
        #[arg(long)]
        to: Option<String>,
        /// Emit the changed lines per page instead of only the first difference.
        #[arg(long)]
        full: bool,
    },
    /// Delete old builds, keeping the newest N and the published one. JSON to stdout.
    Gc {
        /// Output folder: builds go to <OUT>/builds/<id>/, the live one is <OUT>/current.
        #[arg(long, default_value = "out")]
        out: PathBuf,
        /// How many of the newest builds to keep (the published build is always kept).
        #[arg(long, default_value_t = 10)]
        keep: usize,
    },
    /// List every route and its expanded pages as JSON (what the MCP `list_routes` tool returns).
    Routes {
        /// Site folder: routes.yaml, data.json, templates/ (and optional site.yaml, static/, translations/).
        #[arg(long, default_value = "site")]
        site: PathBuf,
    },
    /// Render one page from live source to stdout — no build, no out dir.
    Render {
        /// URL path, e.g. /books/dune/
        path: String,
        /// Site folder: routes.yaml, data.json, templates/ (and optional site.yaml, static/, translations/).
        #[arg(long, default_value = "site")]
        site: PathBuf,
    },
    /// Webhook hint listener: POST /hint runs the verified build+check pipeline, rate-limited.
    /// Hints are never commands — the request body is ignored, failed checks never publish.
    Events {
        /// Site folder: routes.yaml, data.json, templates/ (and optional site.yaml, static/, translations/).
        #[arg(long, default_value = "site")]
        site: PathBuf,
        /// Output folder: builds go to <OUT>/builds/<id>/, the live one is <OUT>/current.
        #[arg(long, default_value = "out")]
        out: PathBuf,
        /// Port on 127.0.0.1.
        #[arg(long, default_value_t = 8081)]
        port: u16,
        /// Minimum seconds between hint-triggered rebuilds.
        #[arg(long, default_value_t = 30)]
        min_interval_secs: u64,
    },
}

fn main() -> Result<()> {
    match Cli::parse().cmd {
        Cmd::Build { site, out } => {
            let r = molde::build(&site, &out)?;
            eprintln!(
                "built {} pages (+{} assets) in {} ms -> {}",
                r.pages,
                r.assets,
                r.ms,
                r.build_dir.display()
            );
            Ok(())
        }
        Cmd::Check { site, out } => {
            let report = molde::run_check(&site, &out)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            if report["verdict"] != "pass" {
                std::process::exit(1);
            }
            Ok(())
        }
        Cmd::Serve {
            out,
            port,
            watch,
            reload,
            site,
        } => {
            let watch = watch.then_some(molde::WatchOpts {
                site: site.unwrap_or_else(|| PathBuf::from("site")),
                out: out.clone(),
                reload,
            });
            molde::serve(&out.join("current"), port, watch)
        }
        Cmd::Mcp { site, out } => molde::mcp::McpServer { site, out }.run_stdio(),
        Cmd::Diff {
            out,
            from,
            to,
            full,
        } => {
            let diff = molde::diff_builds(&out, from.as_deref(), to.as_deref(), full)?;
            println!("{}", serde_json::to_string_pretty(&diff)?);
            Ok(())
        }
        Cmd::Gc { out, keep } => {
            let gc = molde::gc_builds(&out, keep)?;
            println!("{}", serde_json::to_string_pretty(&gc)?);
            Ok(())
        }
        Cmd::Routes { site } => {
            println!(
                "{}",
                serde_json::to_string_pretty(&molde::mcp::list_routes(&site)?)?
            );
            Ok(())
        }
        Cmd::Render { path, site } => {
            print!("{}", molde::render_url(&site, &path)?);
            Ok(())
        }
        Cmd::Events {
            site,
            out,
            port,
            min_interval_secs,
        } => std::sync::Arc::new(molde::events::EventsServer::new(
            site,
            out,
            std::time::Duration::from_secs(min_interval_secs),
        ))
        .run(port),
    }
}

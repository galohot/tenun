mod agent;

use clap::{Parser, Subcommand};
use std::net::IpAddr;
use std::path::PathBuf;

/// Tenun — weave your spreadsheets and databases into one dashboard.
#[derive(Parser)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Run Tenun in your browser (the default).
    Serve {
        /// A .tenun workspace to open, or a new one to create.
        file: Option<PathBuf>,
        /// Port to listen on.
        #[arg(long, default_value_t = 4310)]
        port: u16,
        /// Address to listen on. Anything other than 127.0.0.1 shares Tenun
        /// on your network and requires the printed access link.
        #[arg(long, default_value = "127.0.0.1")]
        host: IpAddr,
        /// Do not open the browser.
        #[arg(long)]
        no_open: bool,
    },
    /// Connect this computer to tenun.cloud (opens a code to approve in a browser).
    Login,
    /// Forget the tenun.cloud connection on this computer.
    Logout,
    /// Publish dashboards to tenun.cloud now.
    Publish {
        /// The .tenun workspace.
        file: PathBuf,
        /// Dashboard id or title (default: every dashboard set to publish).
        #[arg(long)]
        dashboard: Option<String>,
    },
    /// Keep published dashboards updated, without the app (for an always-on computer).
    Agent {
        /// The .tenun workspace.
        file: PathBuf,
    },
    /// Build a dashboard's snapshot without the app: one HTML file, or the JSON data.
    Snapshot {
        /// The .tenun workspace.
        file: PathBuf,
        /// Dashboard id or title (default: the first dashboard).
        #[arg(long)]
        dashboard: Option<String>,
        /// Write a self-contained HTML file here.
        #[arg(long)]
        html: Option<PathBuf>,
        /// Write the snapshot JSON here (default: standard output).
        #[arg(long)]
        json: Option<PathBuf>,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "tenun=info,tenun_core=info".into()),
        )
        .init();
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Snapshot {
            file,
            dashboard,
            html,
            json,
        }) => return snapshot_cmd(file, dashboard, html, json),
        Some(Command::Login) => return agent::login(),
        Some(Command::Logout) => return agent::logout(),
        Some(Command::Publish { file, dashboard }) => {
            return agent::publish_now(&file, dashboard.as_deref());
        }
        Some(Command::Agent { file }) => return agent::run(&file),
        _ => {}
    }
    let (file, port, host, no_open) = match cli.command {
        Some(Command::Serve {
            file,
            port,
            host,
            no_open,
        }) => (file, port, host, no_open),
        _ => (None, 4310, IpAddr::from([127, 0, 0, 1]), false),
    };
    let opts = |port| tenun::Options {
        host,
        port,
        workspace: file.clone(),
        picker: None,
    };
    // If the default port is taken (a second copy), take any free port.
    let running = match tenun::start(opts(port)).await {
        Ok(r) => r,
        Err(e) if port == 4310 && e.to_string().contains("in use") => tenun::start(opts(0)).await?,
        Err(e) => return Err(e),
    };
    println!("Tenun {} is running at {}", tenun::VERSION, running.url);
    if !host.is_loopback() {
        println!(
            "Anyone with this link on your network can use Tenun and read files you can read."
        );
    }
    println!("Press Ctrl+C to stop.");
    if !no_open {
        let _ = webbrowser::open(&running.url);
    }
    tokio::signal::ctrl_c().await?;
    Ok(())
}

fn snapshot_cmd(
    file: PathBuf,
    dashboard: Option<String>,
    html: Option<PathBuf>,
    json: Option<PathBuf>,
) -> anyhow::Result<()> {
    use tenun_core::Engine;
    use tenun_core::secrets::Secrets;
    let engine = Engine::new(Secrets::new(Secrets::default_path()))?;
    engine.open(&file)?;
    let snap = engine.snapshot();
    if let Some((id, e)) = snap.errors.iter().next() {
        anyhow::bail!("source {id} did not load: {e}");
    }
    // A calculated column that cannot be computed is blank and does not stop the snapshot. The
    // words name the table and the column only, as in the pages it makes (V4-D77).
    for problem in engine.public_table_problems() {
        eprintln!("warning: {problem}");
    }
    let dashboards = &snap.workspace.dashboards;
    let pick = match &dashboard {
        Some(want) => dashboards.iter().find(|d| {
            d.get("id").and_then(|v| v.as_str()) == Some(want.as_str())
                || d.get("title").and_then(|v| v.as_str()) == Some(want.as_str())
        }),
        None => dashboards.first(),
    }
    .ok_or_else(|| anyhow::anyhow!("no such dashboard in {}", file.display()))?;
    let id = pick
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let payload = tenun_core::snapshot::build(&engine, &id, &[])?;
    let to_stdout = html.is_none() && json.is_none();
    if let Some(p) = html {
        let mut payload = payload.clone();
        tenun::add_maps(&mut payload, &engine.geo_dir());
        std::fs::write(&p, tenun::export_html(&payload))?;
        eprintln!("wrote {}", p.display());
    }
    match json {
        Some(p) => {
            std::fs::write(&p, serde_json::to_string(&payload)?)?;
            eprintln!("wrote {}", p.display());
        }
        None if to_stdout => println!("{payload}"),
        None => {}
    }
    Ok(())
}

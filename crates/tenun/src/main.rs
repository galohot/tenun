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
    let (file, port, host, no_open) = match cli.command {
        Some(Command::Serve {
            file,
            port,
            host,
            no_open,
        }) => (file, port, host, no_open),
        None => (None, 4310, IpAddr::from([127, 0, 0, 1]), false),
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

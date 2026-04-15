//! Google Docs MCP Server for Noema - Standalone Binary
//!
//! This binary provides an HTTP MCP server for Google Docs.
//! For embedded use, import the library directly.

use clap::Parser;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Port to listen on (0 for random)
    #[arg(short, long, default_value_t = 0)]
    port: u16,

    /// Host to bind to
    #[arg(short = 'H', long, default_value = "127.0.0.1")]
    host: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Log to file if GDOCS_LOG_FILE is set, otherwise stderr
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "mcp_gdocs=info".into());

    if let Ok(log_path) = std::env::var("GDOCS_LOG_FILE") {
        let file = std::fs::File::create(&log_path)?;
        tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .with_writer(file)
            .with_ansi(false)
            .init();
        eprintln!("Logging to {log_path}");
    } else {
        tracing_subscriber::fmt()
            .with_env_filter(env_filter)
            .init();
    };

    tracing::info!("mcp-gdocs starting");

    // Start the server
    let handle = mcp_gdocs::start_server_on(&args.host, args.port).await?;

    eprintln!("Google Docs MCP server running at {}", handle.url());
    tracing::info!(url = %handle.url(), "server ready");

    // Wait for shutdown signal
    tokio::signal::ctrl_c().await?;

    handle.stop();
    Ok(())
}

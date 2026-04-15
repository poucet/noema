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

    /// Log level
    #[arg(short, long, default_value = "info")]
    log_level: String,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    // Set up logging — use RUST_LOG env var if set, otherwise CLI arg
    let env_filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| {
            let level = &args.log_level;
            tracing_subscriber::EnvFilter::new(format!("mcp_gdocs={level}"))
        });

    tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .init();

    // Start the server
    let handle = noema_mcp_gdocs::start_server_on(&args.host, args.port).await?;

    println!("Google Docs MCP server running at {}", handle.url());
    println!("Press Ctrl+C to stop");

    // Wait for shutdown signal
    tokio::signal::ctrl_c().await?;

    handle.stop();
    Ok(())
}

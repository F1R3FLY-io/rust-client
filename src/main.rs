use clap::Parser;
use node_cli::args::Cli;
use node_cli::dispatcher::Dispatcher;
use node_cli::error::Result;

#[tokio::main]
async fn main() -> Result<()> {
 tracing_subscriber::fmt()
 .with_env_filter(
 tracing_subscriber::EnvFilter::from_default_env()
 .add_directive(tracing::Level::WARN.into()),
 )
 .init();

 let cli = Cli::parse();
 Dispatcher::dispatch(&cli).await
}

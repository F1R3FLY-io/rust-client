use node_cli::dispatcher::Dispatcher;
use node_cli::settings::{self, ParseError};
use node_cli::utils::print_error;
use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::WARN.into()),
        )
        .init();

    let parsed = match settings::parse(std::env::args_os().collect()) {
        Ok(parsed) => parsed,
        Err(ParseError::Usage(error)) => error.exit(),
        Err(ParseError::Config(error)) => {
            print_error(&error.to_string());
            return ExitCode::FAILURE;
        }
    };
    // The dispatcher has already printed the error.
    match Dispatcher::dispatch(&parsed).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::FAILURE,
    }
}

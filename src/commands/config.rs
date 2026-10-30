use std::collections::HashMap;

use crate::args::ConfigShowArgs;
use crate::commands::CliResult;
use crate::endpoint::Endpoint;
use crate::settings::SelectedProfile;
use crate::utils::CryptoUtils;

/// Print every setting, its resolved value and its source, and fail if any of
/// them cannot be used, such as a CA certificate with a plaintext endpoint or
/// a key file others can read. The signing key is shown by its vault address.
pub fn config_show_command(
    args: &ConfigShowArgs,
    profile: Option<&SelectedProfile>,
    sources: &HashMap<String, String>,
) -> CliResult {
    let source = |id: &str| sources.get(id).map(String::as_str).unwrap_or("not set");
    let row = |label: &str, value: &str, id: &str| match value {
        "not set" => println!("  {label:<16} not set"),
        _ => println!("  {label:<16} {value:<44} ({})", source(id)),
    };
    let mut problems = Vec::new();

    match profile {
        Some(profile) => println!("Profile: {} ({})", profile.name, profile.file.display()),
        None => println!("Profile: none"),
    }
    println!();

    let endpoints: [(&str, &str, Option<&Endpoint>); 6] = [
        ("grpc", "grpc", Some(&args.grpc)),
        ("http", "http", Some(&args.http)),
        ("observer", "observer", args.observer.as_ref()),
        ("internal grpc", "internal_grpc", Some(&args.internal_grpc)),
        (
            "read-only grpc",
            "readonly_grpc",
            args.readonly_grpc.as_ref(),
        ),
        (
            "read-only http",
            "readonly_http",
            args.readonly_http.as_ref(),
        ),
    ];
    for (label, id, endpoint) in endpoints {
        match endpoint {
            Some(endpoint) => {
                row(label, &endpoint.to_string(), id);
                if let Err(error) = args.transport.apply(endpoint) {
                    problems.push(format!("{label}: {error}"));
                }
            }
            None => row(label, "not set", id),
        }
    }

    let ca_cert = args
        .transport
        .ca_cert
        .as_ref()
        .map(|path| path.display().to_string());
    row(
        "ca cert",
        ca_cert.as_deref().unwrap_or("not set"),
        "ca_cert",
    );
    row(
        "connect timeout",
        &format!("{}s", args.transport.connect_timeout),
        "connect_timeout",
    );
    row(
        "request timeout",
        &format!("{}s", args.transport.timeout),
        "timeout",
    );
    row("shard id", &args.shard_id, "shard_id");

    let key_source = match &args.key.private_key_file {
        Some(path) => format!("file {}", path.display()),
        None if std::env::var_os("FIREFLY_PRIVATE_KEY").is_some() => {
            "FIREFLY_PRIVATE_KEY".to_string()
        }
        None => "not set".to_string(),
    };
    let key_origin = if args.key.private_key_file.is_some() {
        source("private_key_file")
    } else {
        "environment"
    };
    match key_source.as_str() {
        "not set" => println!("  {:<16} not set", "signing key"),
        _ => println!("  {:<16} {key_source:<44} ({key_origin})", "signing key"),
    }
    match args.key.load_optional() {
        Ok(Some(key)) => {
            let address = CryptoUtils::generate_vault_address(&hex::encode(
                key.public_key().serialize_uncompressed(),
            ))?;
            println!("  {:<16} {address}", "vault address");
        }
        Ok(None) => {}
        Err(error) => problems.push(format!("signing key: {error}")),
    }

    if problems.is_empty() {
        return Ok(());
    }
    println!();
    for problem in &problems {
        println!("  {problem}");
    }
    Err(format!("{} setting(s) cannot be used", problems.len()).into())
}

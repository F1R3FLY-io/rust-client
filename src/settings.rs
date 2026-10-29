//! The CLI's settings: a config file of named profiles, layered under the
//! environment and flags.
//!
//! A profile supplies defaults for the arguments it names. Parsing then
//! resolves each argument as clap does: a flag, else its environment variable,
//! else the profile's value, else the built-in default.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

use clap::parser::ValueSource;
use clap::{ArgMatches, CommandFactory, FromArgMatches};
use serde::Deserialize;

use crate::args::Cli;
use crate::commands::CliResult;

/// A config file: profiles by name, and the one used when none is selected.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigFile {
    default_profile: Option<String>,
    #[serde(default)]
    profiles: BTreeMap<String, Profile>,
}

/// One shard's settings. Each field is the default of the argument with the
/// same id; relative paths are relative to the config file.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Profile {
    grpc: Option<String>,
    http: Option<String>,
    internal_grpc: Option<String>,
    observer: Option<String>,
    readonly_grpc: Option<String>,
    readonly_http: Option<String>,
    ca_cert: Option<PathBuf>,
    connect_timeout: Option<u64>,
    timeout: Option<u64>,
    shard_id: Option<String>,
    private_key_file: Option<PathBuf>,
}

/// The profile in effect and the file it came from.
#[derive(Debug, Clone)]
pub struct SelectedProfile {
    pub name: String,
    pub file: PathBuf,
}

/// The parsed command line, with where `config show`'s values came from.
pub struct Parsed {
    pub cli: Cli,
    pub profile: Option<SelectedProfile>,
    /// For `config show`: each argument id and the source of its value.
    pub sources: HashMap<String, String>,
}

/// Why the command line could not be resolved.
pub enum ParseError {
    /// Usage errors, `--help` and `--version`, which clap reports itself.
    Usage(clap::Error),
    Config(crate::commands::CliError),
}

impl From<clap::Error> for ParseError {
    fn from(error: clap::Error) -> Self {
        ParseError::Usage(error)
    }
}

/// Resolve `args`, the full command line including the program name.
pub fn parse(args: Vec<OsString>) -> Result<Parsed, ParseError> {
    // Read --config and --profile alone; the rest can only be checked once
    // the profile has supplied its defaults.
    let early = Cli::command()
        .ignore_errors(true)
        .try_get_matches_from(args.clone())?;
    let config = early.get_one::<PathBuf>("config").cloned();
    let profile_name = early.get_one::<String>("profile").cloned();

    let loaded = load(config.as_deref(), profile_name.as_deref()).map_err(ParseError::Config)?;
    let defaults = match &loaded {
        Some((selected, profile)) => {
            profile_defaults(profile, &selected.file).map_err(ParseError::Config)?
        }
        None => Vec::new(),
    };

    let matches = with_defaults(Cli::command(), &defaults).try_get_matches_from(args)?;
    let cli = Cli::from_arg_matches(&matches)?;
    let selected = loaded.map(|(selected, _)| selected);
    let injected: HashSet<&str> = defaults.iter().map(|(id, _)| *id).collect();
    let sources = config_show_matches(&matches)
        .map(|show| value_sources(show, &injected, selected.as_ref()))
        .unwrap_or_default();

    Ok(Parsed {
        cli,
        profile: selected,
        sources,
    })
}

/// `$XDG_CONFIG_HOME/f1r3fly/node_cli.toml`, else `~/.config/f1r3fly/node_cli.toml`.
fn default_config_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(base.join("f1r3fly").join("node_cli.toml"))
}

/// The selected profile, if any. An explicit `--config` must exist, and a
/// named profile must be in the file; only the default location may be
/// absent.
fn load(
    config: Option<&Path>,
    profile: Option<&str>,
) -> CliResult<Option<(SelectedProfile, Profile)>> {
    let (path, explicit) = match config {
        Some(path) => (path.to_path_buf(), true),
        None => match default_config_path() {
            Some(path) => (path, false),
            None if profile.is_some() => {
                return Err("--profile needs a config file: pass --config".into())
            }
            None => return Ok(None),
        },
    };
    if !path.exists() {
        if explicit || profile.is_some() {
            return Err(format!("config file {} does not exist", path.display()).into());
        }
        return Ok(None);
    }

    let text = std::fs::read_to_string(&path).map_err(|cause| crate::Error::Io {
        path: path.clone(),
        cause,
    })?;
    let mut file: ConfigFile =
        toml::from_str(&text).map_err(|e| format!("config file {}: {e}", path.display()))?;

    let Some(name) = profile.map(str::to_string).or(file.default_profile.clone()) else {
        return Ok(None);
    };
    let Some(selected) = file.profiles.remove(&name) else {
        let known: Vec<&str> = file.profiles.keys().map(String::as_str).collect();
        return Err(format!(
            "config file {} has no profile {name:?}; it has: {}",
            path.display(),
            if known.is_empty() {
                "none".to_string()
            } else {
                known.join(", ")
            }
        )
        .into());
    };
    Ok(Some((SelectedProfile { name, file: path }, selected)))
}

/// The profile's values as `(argument id, default)` pairs.
fn profile_defaults(profile: &Profile, file: &Path) -> CliResult<Vec<(&'static str, String)>> {
    let dir = file.parent().unwrap_or(Path::new("."));
    let path = |path: &Path| -> CliResult<String> {
        let resolved = match path.strip_prefix("~") {
            Ok(rest) => PathBuf::from(
                std::env::var_os("HOME")
                    .ok_or("a profile path starts with ~ but HOME is not set")?,
            )
            .join(rest),
            Err(_) if path.is_relative() => dir.join(path),
            Err(_) => path.to_path_buf(),
        };
        resolved
            .into_os_string()
            .into_string()
            .map_err(|p| format!("profile path {p:?} is not unicode").into())
    };

    let mut defaults = Vec::new();
    for (id, value) in [
        ("grpc", &profile.grpc),
        ("http", &profile.http),
        ("internal_grpc", &profile.internal_grpc),
        ("observer", &profile.observer),
        ("readonly_grpc", &profile.readonly_grpc),
        ("readonly_http", &profile.readonly_http),
        ("shard_id", &profile.shard_id),
    ] {
        if let Some(value) = value {
            defaults.push((id, value.clone()));
        }
    }
    for (id, value) in [
        ("connect_timeout", profile.connect_timeout),
        ("timeout", profile.timeout),
    ] {
        if let Some(value) = value {
            defaults.push((id, value.to_string()));
        }
    }
    for (id, value) in [
        ("ca_cert", &profile.ca_cert),
        ("private_key_file", &profile.private_key_file),
    ] {
        if let Some(value) = value {
            defaults.push((id, path(value)?));
        }
    }
    Ok(defaults)
}

/// `command` with each defaulted argument, in every subcommand that has it,
/// taking the profile's value as its default. A profile value also satisfies
/// an argument that is otherwise required.
fn with_defaults(mut command: clap::Command, defaults: &[(&'static str, String)]) -> clap::Command {
    for (id, value) in defaults {
        if command.get_arguments().any(|arg| arg.get_id() == id) {
            let value = value.clone();
            command = command.mut_arg(*id, |arg| arg.default_value(value).required(false));
        }
    }
    let names: Vec<String> = command
        .get_subcommands()
        .map(|sub| sub.get_name().to_string())
        .collect();
    for name in names {
        command = command.mut_subcommand(name, |sub| with_defaults(sub, defaults));
    }
    command
}

fn config_show_matches(matches: &ArgMatches) -> Option<&ArgMatches> {
    matches
        .subcommand_matches("config")?
        .subcommand_matches("show")
}

/// Where each of `config show`'s values came from.
fn value_sources(
    matches: &ArgMatches,
    injected: &HashSet<&str>,
    profile: Option<&SelectedProfile>,
) -> HashMap<String, String> {
    matches
        .ids()
        .map(|id| {
            let id = id.as_str();
            let source = match matches.value_source(id) {
                Some(ValueSource::CommandLine) => "flag".to_string(),
                Some(ValueSource::EnvVariable) => "environment".to_string(),
                Some(ValueSource::DefaultValue) => match (injected.contains(id), profile) {
                    (true, Some(profile)) => format!("profile {}", profile.name),
                    _ => "default".to_string(),
                },
                _ => "not set".to_string(),
            };
            (id.to_string(), source)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::args::Commands;

    fn config(text: &str) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "node-cli-config-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("node_cli.toml");
        std::fs::write(&file, text).unwrap();
        (dir, file)
    }

    /// Clear the variables that would otherwise outrank a profile, so an
    /// exported one in the shell running the tests cannot change them.
    fn isolate() {
        for var in [
            "FIREFLY_CONFIG",
            "FIREFLY_PROFILE",
            "FIREFLY_GRPC_URL",
            "FIREFLY_HTTP_URL",
            "FIREFLY_OBSERVER_URL",
            "FIREFLY_INTERNAL_GRPC_URL",
            "FIREFLY_READONLY_GRPC_URL",
            "FIREFLY_READONLY_HTTP_URL",
            "FIREFLY_CA_CERT",
            "FIREFLY_CONNECT_TIMEOUT",
            "FIREFLY_REQUEST_TIMEOUT",
            "FIREFLY_SHARD_ID",
            "FIREFLY_PRIVATE_KEY_FILE",
        ] {
            std::env::remove_var(var);
        }
    }

    #[test]
    fn every_profile_field_names_an_argument_resolved_the_same_everywhere() {
        let profile = Profile {
            grpc: Some("http://a:1".into()),
            http: Some("http://a:2".into()),
            internal_grpc: Some("http://a:3".into()),
            observer: Some("http://a:4".into()),
            readonly_grpc: Some("http://a:5".into()),
            readonly_http: Some("http://a:6".into()),
            ca_cert: Some("ca.pem".into()),
            connect_timeout: Some(1),
            timeout: Some(2),
            shard_id: Some("s".into()),
            private_key_file: Some("key.hex".into()),
        };
        let ids: Vec<&str> = profile_defaults(&profile, Path::new("/config/node_cli.toml"))
            .unwrap()
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        assert_eq!(
            ids.len(),
            11,
            "a profile field is not turned into a default"
        );

        type Resolution = (Option<String>, Vec<String>);
        fn walk(command: &clap::Command, found: &mut HashMap<String, Vec<(String, Resolution)>>) {
            for arg in command.get_arguments() {
                let resolution = (
                    arg.get_env().map(|env| env.to_string_lossy().into_owned()),
                    arg.get_default_values()
                        .iter()
                        .map(|value| value.to_string_lossy().into_owned())
                        .collect(),
                );
                found
                    .entry(arg.get_id().to_string())
                    .or_default()
                    .push((command.get_name().to_string(), resolution));
            }
            for sub in command.get_subcommands() {
                walk(sub, found);
            }
        }
        let mut found = HashMap::new();
        walk(&Cli::command(), &mut found);

        for id in ids {
            let occurrences = found
                .get(id)
                .unwrap_or_else(|| panic!("profile field {id} matches no argument"));
            let (_, first) = &occurrences[0];
            for (command, resolution) in occurrences {
                assert_eq!(
                    resolution, first,
                    "{id} resolves differently in {command} than in {}",
                    occurrences[0].0
                );
            }
        }
    }

    fn parse_with(file: &Path, args: &[&str]) -> Result<Parsed, ParseError> {
        isolate();
        let mut argv: Vec<OsString> = vec!["node_cli".into(), "--config".into(), file.into()];
        argv.extend(args.iter().map(OsString::from));
        parse(argv)
    }

    const SHARD: &str = r#"
        default_profile = "shard"

        [profiles.shard]
        grpc = "https://validator.example:40401"
        http = "https://validator.example:40403"
        readonly_http = "https://observer.example:40403"
        ca_cert = "ca.pem"
        timeout = 60
    "#;

    #[test]
    fn a_flag_beats_the_profile_which_beats_the_built_in_default() {
        let (dir, file) = config(SHARD);
        let parsed = parse_with(
            &file,
            &[
                "config",
                "show",
                "--http",
                "https://elsewhere.example:40403",
            ],
        )
        .unwrap_or_else(|_| panic!("parse"));
        let Commands::Config(config) = &parsed.cli.command else {
            unreachable!()
        };
        let crate::args::ConfigAction::Show(show) = &config.action;

        assert_eq!(show.http.to_string(), "https://elsewhere.example:40403");
        assert_eq!(parsed.sources["http"], "flag");
        assert_eq!(show.grpc.to_string(), "https://validator.example:40401");
        assert_eq!(parsed.sources["grpc"], "profile shard");
        assert_eq!(show.transport.timeout, 60);
        assert_eq!(show.transport.connect_timeout, 10);
        assert_eq!(parsed.sources["connect_timeout"], "default");
        assert_eq!(
            show.transport.ca_cert.as_deref(),
            Some(dir.join("ca.pem").as_path())
        );
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_profile_supplies_an_endpoint_a_command_requires() {
        let (dir, file) = config(SHARD);
        let parsed = parse_with(&file, &["bonds"]).unwrap_or_else(|_| panic!("parse"));
        let Commands::Bonds(bonds) = &parsed.cli.command else {
            unreachable!()
        };
        assert_eq!(bonds.http.to_string(), "https://observer.example:40403");
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_config_mistake_is_an_error_not_a_fallback() {
        let (dir, file) = config("[profiles.shard]\ngrcp = \"http://typo:40401\"\n");
        assert!(matches!(
            parse_with(&file, &["--profile", "shard", "status"]),
            Err(ParseError::Config(e)) if e.to_string().contains("unknown field `grcp`")
        ));

        let (_, file) = config(SHARD);
        assert!(matches!(
            parse_with(&file, &["--profile", "other", "status"]),
            Err(ParseError::Config(e)) if e.to_string().contains("has no profile \"other\"")
        ));
        assert!(matches!(
            parse_with(&dir.join("missing.toml"), &["status"]),
            Err(ParseError::Config(e)) if e.to_string().contains("does not exist")
        ));
        std::fs::remove_dir_all(dir).unwrap();
    }
}

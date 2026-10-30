use crate::args::*;
use crate::commands::*;
use crate::error::Error;
use crate::settings::Parsed;
use crate::utils::print_error;

/// Central command dispatcher that routes and executes all CLI commands
pub struct Dispatcher;

impl Dispatcher {
    /// Run the command, printing its error, if any, with a suggestion.
    pub async fn dispatch(parsed: &Parsed) -> CliResult {
        let result = match &parsed.cli.command {
            Commands::Deploy(args) => deploy_command(args).await,
            Commands::Propose(args) => propose_command(args).await,
            Commands::DeployAndWait(args) => deploy_and_wait_command(args).await,
            Commands::GetData(args) => get_data_command(args).await,
            Commands::IsFinalized(args) => is_finalized_command(args).await,
            Commands::ExploratoryDeploy(args) => exploratory_deploy_command(args).await,
            Commands::EstimateCost(args) => estimate_cost_command(args).await,
            Commands::GeneratePublicKey(args) => generate_public_key_command(args),
            Commands::GenerateKeyPair(args) => generate_key_pair_command(args),
            Commands::GenerateVaultAddress(args) => generate_vault_address_command(args),
            Commands::Status(args) => status_command(args).await,
            Commands::Blocks(args) => blocks_command(args).await,
            Commands::Bonds(args) => bonds_command(args).await,
            Commands::ActiveValidators(args) => active_validators_command(args).await,
            Commands::WalletBalance(args) => wallet_balance_command(args).await,
            Commands::BondStatus(args) => bond_status_command(args).await,
            Commands::Metrics(args) => metrics_command(args).await,
            Commands::BondValidator(args) => bond_validator_command(args).await,
            Commands::UnbondValidator(args) => unbond_validator_command(args).await,
            Commands::NetworkHealth(args) => network_health_command(args).await,
            Commands::LastFinalizedBlock(args) => last_finalized_block_command(args).await,
            Commands::ShowMainChain(args) => show_main_chain_command(args).await,
            Commands::Transfer(args) => transfer_command(args).await,
            Commands::LoadTest(args) => load_test_command(args).await,
            Commands::GetDeploy(args) => get_deploy_command(args).await,
            Commands::EpochInfo(args) => epoch_info_command(args).await,
            Commands::ValidatorStatus(args) => validator_status_command(args).await,
            Commands::EpochRewards(args) => epoch_rewards_command(args).await,
            Commands::NetworkConsensus(args) => network_consensus_command(args).await,
            Commands::GetBlocksByHeight(args) => get_blocks_by_height_command(args).await,
            Commands::GetNodeId(args) => get_node_id_command(args),
            Commands::WatchEvents(args) => watch_events_command(args).await,
            Commands::Dag(args) => run_dag(args).await,
            Commands::BlockTransfers(args) => block_transfers_command(args).await,
            Commands::DeployStatus(args) => deploy_status_command(args).await,
            Commands::Config(ConfigArgs {
                action: ConfigAction::Show(args),
            }) => config_show_command(args, parsed.profile.as_ref(), &parsed.sources),
        };

        if let Err(error) = &result {
            print_error(&error.to_string());
            if let Some(suggestion) = error.downcast_ref::<Error>().and_then(suggestion) {
                eprintln!(" Suggestion: {suggestion}");
            }
        }
        result
    }
}

fn suggestion(error: &Error) -> Option<&'static str> {
    match error {
        Error::Transport { .. } => {
            Some("check that the node is running and that the endpoint URL is right")
        }
        Error::Http {
            code: Some(code), ..
        } if code == "readonly_node_required" => {
            Some("this query needs a read-only node; point it at the observer")
        }
        Error::InvalidInput { .. } | Error::MissingConfig { .. } => {
            Some("check the command's arguments and environment")
        }
        Error::NodeTooOld { .. } => Some("upgrade the node, or point the command at a newer one"),
        _ => None,
    }
}

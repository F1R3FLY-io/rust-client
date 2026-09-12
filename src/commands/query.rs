use crate::args::*;
use crate::f1r3fly_api::F1r3flyApi;
use crate::pos::{parse_pos_snapshot, PosSnapshot, POS_SNAPSHOT_QUERY};
use crate::utils::http::EXPLORATORY_RETRY_BACKOFF;
use crate::utils::{build_url, HttpClient};
use reqwest;
use serde_json;
use std::collections::{HashSet, VecDeque};
use std::time::Instant;

pub async fn status_command(args: &HttpArgs) -> Result<(), Box<dyn std::error::Error>> {
    println!(" Getting node status from {}:{}", args.host, args.port);

    let url = format!("http://{}:{}/api/status", args.host, args.port);
    let client = reqwest::Client::new();

    let start_time = Instant::now();

    match client.get(&url).send().await {
        Ok(response) => {
            let duration = start_time.elapsed();
            if response.status().is_success() {
                let status_text = response.text().await?;
                let status: crate::f1r3fly_api::NodeStatus = serde_json::from_str(&status_text)?;

                println!(" Node status retrieved successfully!");
                println!(" Time taken: {duration:.2?}");
                println!();
                println!("  Address:       {}", status.address);
                println!("  Network:       {}", status.network_id);
                println!("  Shard:         {}", status.shard_id);
                println!("  Peers:         {}", status.peers);
                println!("  Nodes:         {}", status.nodes);
                println!("  Min Phlo:      {}", status.min_phlo_price);
                if !status.native_token_name.is_empty() {
                    println!(
                        "  Native Token:  {} ({}, {} decimals)",
                        status.native_token_name,
                        status.native_token_symbol,
                        fmt(status.native_token_decimals)
                    );
                }
                fn fmt<T: std::fmt::Display>(v: Option<T>) -> String {
                    v.map(|x| x.to_string()).unwrap_or_else(|| "N/A".into())
                }
                println!(
                    "  LFB Number:    {}",
                    fmt(status.last_finalized_block_number)
                );
                println!("  Validator:     {}", fmt(status.is_validator));
                println!("  Read Only:     {}", fmt(status.is_read_only));
                println!("  Ready:         {}", fmt(status.is_ready));
                println!(
                    "  Epoch:         {} (length: {})",
                    fmt(status.current_epoch),
                    fmt(status.epoch_length)
                );
                println!("  Version:       {}", status.version);
            } else {
                println!(" Failed to get node status: HTTP {}", response.status());
                println!("Error: {}", response.text().await?);
            }
        }
        Err(e) => {
            println!(" Connection failed!");
            println!("Error: {e}");
            return Err(e.into());
        }
    }

    Ok(())
}

pub async fn blocks_command(args: &BlocksArgs) -> Result<(), Box<dyn std::error::Error>> {
    let start_time = Instant::now();
    let client = reqwest::Client::new();

    if let Some(block_hash) = &args.block_hash {
        println!(" Getting specific block: {block_hash}");
        let url = format!(
            "http://{}:{}/api/block/{}",
            args.host, args.port, block_hash
        );

        match client.get(&url).send().await {
            Ok(response) => {
                let duration = start_time.elapsed();
                if response.status().is_success() {
                    let block_text = response.text().await?;
                    let block_json: serde_json::Value = serde_json::from_str(&block_text)?;

                    println!(" Block retrieved successfully!");
                    println!(" Time taken: {duration:.2?}");
                    println!(" Block Details:");
                    println!("{}", serde_json::to_string_pretty(&block_json)?);
                } else {
                    println!(" Failed to get block: HTTP {}", response.status());
                    println!("Error: {}", response.text().await?);
                }
            }
            Err(e) => {
                println!(" Connection failed!");
                println!("Error: {e}");
                return Err(e.into());
            }
        }
    } else {
        println!(
            " Getting {} recent blocks from {}:{}",
            args.number, args.host, args.port
        );
        let url = format!(
            "http://{}:{}/api/blocks/{}",
            args.host, args.port, args.number
        );

        match client.get(&url).send().await {
            Ok(response) => {
                let duration = start_time.elapsed();
                if response.status().is_success() {
                    let blocks_text = response.text().await?;
                    let blocks_json: serde_json::Value = serde_json::from_str(&blocks_text)?;

                    println!(" Blocks retrieved successfully!");
                    println!(" Time taken: {duration:.2?}");
                    println!(" Recent Blocks:");
                    println!("{}", serde_json::to_string_pretty(&blocks_json)?);
                } else {
                    println!(" Failed to get blocks: HTTP {}", response.status());
                    println!("Error: {}", response.text().await?);
                }
            }
            Err(e) => {
                println!(" Connection failed!");
                println!("Error: {e}");
                return Err(e.into());
            }
        }
    }

    Ok(())
}

pub async fn bonds_command(args: &HttpArgs) -> Result<(), Box<dyn std::error::Error>> {
    println!(" Getting validator bonds from {}:{}", args.host, args.port);

    let start_time = Instant::now();
    let snapshot = fetch_pos_snapshot(&args.host, args.port).await?;

    println!(" Validator bonds retrieved successfully!");
    println!(" Time taken: {:.2?}", start_time.elapsed());
    println!();

    let total_stake: i64 = snapshot.bonds.values().sum();
    println!(
        " Bonded Validators ({} total, {total_stake} total stake):",
        snapshot.bonds.len()
    );
    println!();

    for (i, (key, stake)) in snapshot.bonds.iter().enumerate() {
        let activation = if snapshot.is_active(key) {
            ""
        } else {
            " (pending activation)"
        };
        println!(
            " {}. {} (stake: {stake}){activation}",
            i + 1,
            abbreviate_key(key)
        );
    }

    Ok(())
}

pub async fn active_validators_command(args: &HttpArgs) -> Result<(), Box<dyn std::error::Error>> {
    println!(
        " Getting active validators from {}:{}",
        args.host, args.port
    );

    let start_time = Instant::now();
    let snapshot = fetch_pos_snapshot(&args.host, args.port).await?;

    println!(" Active validators retrieved successfully!");
    println!(" Time taken: {:.2?}", start_time.elapsed());
    println!();

    let total_stake: i64 = snapshot
        .active
        .iter()
        .filter_map(|key| snapshot.stake(key))
        .sum();
    println!(
        " Active Validators ({} total, {total_stake} total stake):",
        snapshot.active.len()
    );
    println!();

    for (i, key) in snapshot.active.iter().enumerate() {
        match snapshot.stake(key) {
            Some(stake) => println!(" {}. {} (stake: {stake})", i + 1, abbreviate_key(key)),
            None => println!(" {}. {} (not bonded)", i + 1, abbreviate_key(key)),
        }
    }

    Ok(())
}

pub async fn wallet_balance_command(
    args: &WalletBalanceArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    println!(" Checking wallet balance for address: {}", args.address);

    // Use F1r3fly API with gRPC (like exploratory-deploy)
    let f1r3fly_api = F1r3flyApi::new_readonly(&args.host, args.port);

    let rholang_query = format!(
        r#"new return, rl(`rho:registry:lookup`), systemVaultCh, vaultCh, balanceCh in {{
 rl!(`rho:vault:system`, *systemVaultCh) |
 for (@(_, SystemVault) <- systemVaultCh) {{
 @SystemVault!("findOrCreate", "{}", *vaultCh) |
 for (@either <- vaultCh) {{
 match either {{
 (true, vault) => {{
 @vault!("balance", *balanceCh) |
 for (@balance <- balanceCh) {{
 return!(balance)
 }}
 }}
 (false, errorMsg) => {{
 return!(errorMsg)
 }}
 }}
 }}
 }}
 }}"#,
        args.address
    );

    let start_time = Instant::now();

    match f1r3fly_api
        .exploratory_deploy(&rholang_query, None, false)
        .await
    {
        Ok((result, block_info, _cost)) => {
            let duration = start_time.elapsed();
            println!("Wallet balance retrieved successfully!");
            println!("Time taken: {duration:.2?}");
            println!("Balance for {}: {}", args.address, result);
            println!("{block_info}");
        }
        Err(e) => {
            println!(" Failed to get wallet balance!");
            println!("Error: {e}");
            return Err(e);
        }
    }

    Ok(())
}

pub async fn bond_status_command(args: &BondStatusArgs) -> Result<(), Box<dyn std::error::Error>> {
    println!(" Checking bond status for public key: {}", args.public_key);

    let url = build_url(
        &args.host,
        args.port,
        &format!("/api/bond-status/{}", args.public_key),
    );
    let (response, duration) = HttpClient::new().get_with_timing(&url).await?;
    let is_bonded = response
        .get("isBonded")
        .and_then(serde_json::Value::as_bool)
        .ok_or_else(|| format!("unexpected bond-status response: {response}"))?;

    println!(" Time taken: {duration:.2?}");
    if is_bonded {
        println!(" Validator is BONDED");
    } else {
        println!(" Validator is NOT BONDED");
    }
    println!(
        " Activation and withdrawal progress: node_cli validator-status -k {}",
        args.public_key
    );

    Ok(())
}

pub async fn metrics_command(args: &HttpArgs) -> Result<(), Box<dyn std::error::Error>> {
    println!(" Getting node metrics from {}:{}", args.host, args.port);

    let url = format!("http://{}:{}/metrics", args.host, args.port);
    let client = reqwest::Client::new();

    let start_time = Instant::now();

    match client.get(&url).send().await {
        Ok(response) => {
            let duration = start_time.elapsed();
            if response.status().is_success() {
                let metrics_text = response.text().await?;

                println!(" Node metrics retrieved successfully!");
                println!(" Time taken: {duration:.2?}");
                println!(" Node Metrics:");

                // Filter and display key metrics
                let lines: Vec<&str> = metrics_text
                    .lines()
                    .filter(|line| {
                        line.contains("peers")
                            || line.contains("blocks")
                            || line.contains("consensus")
                            || line.contains("casper")
                            || line.contains("rspace")
                    })
                    .collect();

                if lines.is_empty() {
                    println!(" All Metrics:");
                    println!("{metrics_text}");
                } else {
                    println!(" Key Metrics (peers, blocks, consensus):");
                    for line in lines {
                        println!("{line}");
                    }
                    println!("\n Use --verbose flag (if implemented) to see all metrics");
                }
            } else {
                println!(" Failed to get metrics: HTTP {}", response.status());
                println!("Error: {}", response.text().await?);
            }
        }
        Err(e) => {
            println!(" Connection failed!");
            println!("Error: {e}");
            return Err(e.into());
        }
    }

    Ok(())
}

// Helper struct for discovered peers
#[derive(Debug, Clone)]
struct DiscoveredPeer {
    address: String,
    node_id: String,
    host: String,
    protocol_port: u16,
    discovery_port: u16,
    connection_status: String,
}

impl DiscoveredPeer {
    fn from_json(json: &serde_json::Value) -> Option<Self> {
        Some(DiscoveredPeer {
            address: json.get("address")?.as_str()?.to_string(),
            node_id: json.get("nodeId")?.as_str()?.to_string(),
            host: json.get("host")?.as_str()?.to_string(),
            protocol_port: json.get("protocolPort")?.as_u64()? as u16,
            discovery_port: json.get("discoveryPort")?.as_u64()? as u16,
            connection_status: json
                .get("connectionStatus")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string(),
        })
    }

    fn uri_key(&self) -> String {
        format!("{}:{}", self.host, self.protocol_port)
    }
}

// Helper function to query a node's status and return full JSON response
pub(crate) async fn query_node_status(
    client: &reqwest::Client,
    host: &str,
    port: u16,
    debug: bool,
) -> Result<(serde_json::Value, String), String> {
    let url = format!("http://{host}:{port}/status");

    if debug {
        println!("\n [DEBUG] HTTP Request:");
        println!(" Method: GET");
        println!(" URL: {url}");
    }

    match client.get(&url).send().await {
        Ok(response) => {
            let status_code = response.status();

            if debug {
                println!(" [DEBUG] HTTP Response:");
                println!(" Status: {status_code}");
                println!(" Headers: {:#?}", response.headers());
            }

            if status_code.is_success() {
                match response.text().await {
                    Ok(status_text) => {
                        if debug {
                            println!(" [DEBUG] Response Body:");
                            if let Ok(pretty) = serde_json::to_string_pretty(
                                &serde_json::from_str::<serde_json::Value>(&status_text)
                                    .unwrap_or(serde_json::json!({})),
                            ) {
                                for line in pretty.lines() {
                                    println!(" {line}");
                                }
                            }
                        }
                        match serde_json::from_str::<serde_json::Value>(&status_text) {
                            Ok(json) => Ok((json, status_text)),
                            Err(_) => Err("Invalid JSON response".to_string()),
                        }
                    }
                    Err(_) => Err("Failed to read response".to_string()),
                }
            } else {
                Err(format!("HTTP {status_code}"))
            }
        }
        Err(e) => {
            if debug {
                println!(" [DEBUG] Error: {e}");
            }
            Err("Connection failed".to_string())
        }
    }
}

// Helper function to extract peer list from status JSON
fn extract_peers(status_json: &serde_json::Value) -> Vec<DiscoveredPeer> {
    let mut peers = Vec::new();

    if let Some(peer_list) = status_json.get("peerList") {
        if let Some(peer_array) = peer_list.as_array() {
            for peer_json in peer_array {
                if let Some(peer) = DiscoveredPeer::from_json(peer_json) {
                    peers.push(peer);
                }
            }
        }
    }

    peers
}

// Display peer details in a formatted way
fn display_peer_info(peer: &DiscoveredPeer, indent: &str) {
    println!("{} Address: {}", indent, peer.address);
    println!("{} Node ID: {}", indent, peer.node_id);
    println!("{} Host: {}", indent, peer.host);
    println!("{} Protocol Port: {}", indent, peer.protocol_port);
    println!("{} Discovery Port: {}", indent, peer.discovery_port);
    println!("{} Status: {}", indent, peer.connection_status);
}

pub async fn network_health_command(
    args: &NetworkHealthArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    // Validate host and ports combination early
    if let Err(e) = validate_host_and_ports(&args.host, &args.custom_ports) {
        println!(" {e}");
        return Err(e.into());
    }

    println!(" Checking F1r3fly network health");

    let mut ports_to_check = Vec::new();

    if args.standard_ports {
        // Standard F1r3fly shard ports from the docker configuration
        ports_to_check.extend_from_slice(&[
            (40403, "Bootstrap"),
            (40413, "Validator1"),
            (40423, "Validator2"),
            (40433, "Validator3"),
            (40453, "Observer"),
        ]);
    }

    // Add custom ports if specified
    if let Some(custom_ports_str) = &args.custom_ports {
        for port_str in custom_ports_str.split(',') {
            if let Ok(port) = port_str.trim().parse::<u16>() {
                ports_to_check.push((port, "Custom"));
            }
        }
    }

    if ports_to_check.is_empty() {
        println!(" No ports specified to check");
        return Ok(());
    }

    let client = reqwest::Client::new();
    let mut healthy_nodes = 0;
    let mut total_nodes = 0;
    let mut all_peer_lists: Vec<Vec<DiscoveredPeer>> = Vec::new();
    let mut node_status_map: Vec<(String, bool, serde_json::Value)> = Vec::new();

    if args.recursive {
        // Recursive peer discovery mode
        println!(
            " Starting recursive peer discovery (max peers: {})\n",
            if args.max_peers <= 0 {
                "unlimited".to_string()
            } else {
                args.max_peers.to_string()
            }
        );

        let mut visited = HashSet::new();
        let mut queue: VecDeque<(String, u16)> = VecDeque::new();
        let mut discovered_peers = Vec::new();

        // Initialize queue with specified ports
        for (port, _) in &ports_to_check {
            let uri_key = format!("{}:{}", args.host, port);
            queue.push_back((args.host.clone(), *port));
            visited.insert(uri_key);
        }

        // Process discovery queue
        while !queue.is_empty() {
            // Check if we've reached the peer limit
            if args.max_peers > 0 && discovered_peers.len() >= args.max_peers as usize {
                println!("\n Reached maximum peer limit of {}", args.max_peers);
                break;
            }

            if let Some((host, port)) = queue.pop_front() {
                total_nodes += 1;
                let uri_key = format!("{host}:{port}");

                print!(" Querying {host}:{port}: ");

                match query_node_status(&client, &host, port, args.debug).await {
                    Ok((status_json, _raw_response)) => {
                        healthy_nodes += 1;
                        println!(" HEALTHY");

                        // Display full response including peer list
                        node_status_map.push((uri_key.clone(), true, status_json.clone()));

                        // Extract peers from this node
                        let peers = extract_peers(&status_json);
                        all_peer_lists.push(peers.clone());

                        if args.verbose {
                            println!(" Peer count: {}", peers.len());
                        }

                        println!(" Peers from this node:");
                        for peer in &peers {
                            let peer_uri = peer.uri_key();
                            if !visited.contains(&peer_uri)
                                && (args.max_peers <= 0
                                    || discovered_peers.len() < args.max_peers as usize)
                            {
                                visited.insert(peer_uri);
                                queue.push_back((peer.host.clone(), peer.protocol_port));
                                discovered_peers.push(peer.clone());
                                print!(
                                    " Added: {} ({}:{})",
                                    peer.node_id, peer.host, peer.protocol_port
                                );
                                if args.verbose {
                                    print!(" [status: {}]", peer.connection_status);
                                }
                                if args.max_peers > 0
                                    && discovered_peers.len() >= args.max_peers as usize
                                {
                                    println!(" [LIMIT REACHED]");
                                    break;
                                }
                                println!();
                            }
                        }
                    }
                    Err(e) => {
                        println!(" {e}");
                        node_status_map.push((uri_key, false, serde_json::json!({})));
                    }
                }
            }
        }

        println!("\n Recursive Discovery Summary:");
        println!(" Healthy nodes: {healthy_nodes}/{total_nodes}");
        println!(" Total discovered peers: {}", discovered_peers.len());
    } else {
        // Standard mode: just query specified ports
        println!(" Checking {} nodes...\n", ports_to_check.len());

        for (port, node_type) in ports_to_check {
            total_nodes += 1;
            let uri_key = format!("{}:{}", args.host, port);

            print!(" {} ({}:{}): ", node_type, args.host, port);

            match query_node_status(&client, &args.host, port, args.debug).await {
                Ok((status_json, _raw_response)) => {
                    healthy_nodes += 1;
                    let peer_count = status_json
                        .get("peers")
                        .and_then(|p| p.as_u64())
                        .unwrap_or(0);

                    println!(" HEALTHY ({peer_count} peers)");

                    // Store the status and peer list
                    node_status_map.push((uri_key, true, status_json.clone()));
                    let peers = extract_peers(&status_json);
                    all_peer_lists.push(peers);

                    if args.verbose {
                        if let Some(peers_from_endpoint) = status_json.get("peers") {
                            println!(" Peers count from endpoint: {peers_from_endpoint}");
                        }
                        if let Some(version) = status_json.get("version") {
                            println!(" Version: {version}");
                        }
                        if let Some(uptime) = status_json.get("uptime") {
                            println!(" Uptime: {uptime}");
                        }
                    }
                }
                Err(e) => {
                    println!(" {e}");
                    node_status_map.push((uri_key, false, serde_json::json!({})));
                }
            }
        }

        println!("\n Network Health Summary:");
        println!(" Healthy nodes: {healthy_nodes}/{total_nodes}");
    }

    // Display detailed peer information for each node
    if !node_status_map.is_empty() && healthy_nodes > 0 && args.verbose {
        println!("\n Detailed Peer Information:\n");
        for (uri, is_healthy, status_json) in &node_status_map {
            if *is_healthy {
                println!(" Node {uri}:");
                let peers = extract_peers(status_json);
                if peers.is_empty() {
                    println!(" No peers discovered");
                } else {
                    for (i, peer) in peers.iter().enumerate() {
                        println!(" Peer {}:", i + 1);
                        display_peer_info(peer, " ");
                        if i < peers.len() - 1 {
                            println!();
                        }
                    }
                }
                println!();
            }
        }
    }

    // Summary statistics
    if healthy_nodes > 0 && !all_peer_lists.is_empty() {
        let total_peer_count: usize = all_peer_lists.iter().map(|p| p.len()).sum();
        let avg_peers = total_peer_count as f64 / all_peer_lists.len() as f64;

        println!(" Peer Statistics:");
        println!(" Total peer entries: {total_peer_count}");
        println!(" Average peers per node: {avg_peers:.1}");

        if args.verbose {
            let mut peer_counts_by_node: Vec<usize> =
                all_peer_lists.iter().map(|p| p.len()).collect();
            peer_counts_by_node.sort();
            if let Some(min) = peer_counts_by_node.first() {
                println!(" Minimum peers on a node: {min}");
            }
            if let Some(max) = peer_counts_by_node.last() {
                println!(" Maximum peers on a node: {max}");
            }

            // Count peer connectivity status
            let connected_peers: usize = all_peer_lists
                .iter()
                .flat_map(|peers| peers.iter())
                .filter(|p| {
                    p.connection_status.to_lowercase().contains("connected")
                        || p.connection_status.to_lowercase().contains("active")
                })
                .count();
            if connected_peers > 0 {
                println!(" Connected peers: {connected_peers}/{total_peer_count}");
            }
        }

        if healthy_nodes == total_nodes {
            println!(" All queried nodes are HEALTHY!");
        } else {
            println!(" Some nodes are unhealthy - check individual node logs");
        }
    } else if healthy_nodes == 0 {
        println!(" No healthy nodes found - check if network is running");
    }

    Ok(())
}

pub async fn last_finalized_block_command(
    args: &HttpArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    println!(
        " Getting last finalized block from {}:{}",
        args.host, args.port
    );

    let url = format!(
        "http://{}:{}/api/last-finalized-block",
        args.host, args.port
    );
    let client = reqwest::Client::new();

    let start_time = Instant::now();

    match client.get(&url).send().await {
        Ok(response) => {
            let duration = start_time.elapsed();
            if response.status().is_success() {
                let block_text = response.text().await?;
                let block_json: serde_json::Value = serde_json::from_str(&block_text)?;

                println!(" Last finalized block retrieved successfully!");
                println!(" Time taken: {duration:.2?}");

                // Extract key information from blockInfo
                let block_info = block_json.get("blockInfo");

                let block_hash = block_info
                    .and_then(|info| info.get("blockHash"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown");

                let block_number = block_info
                    .and_then(|info| info.get("blockNumber"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);

                let timestamp = block_info
                    .and_then(|info| info.get("timestamp"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);

                // Get deploy count from blockInfo (it's already calculated)
                let deploy_count = block_info
                    .and_then(|info| info.get("deployCount"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);

                let shard_id = block_info
                    .and_then(|info| info.get("shardId"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown");

                let fault_tolerance = block_info
                    .and_then(|info| info.get("faultTolerance"))
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0);

                println!(" Last Finalized Block Summary:");
                println!(" Block Number: {block_number}");
                println!(" Block Hash: {block_hash}");
                println!(" Timestamp: {timestamp}");
                println!(" Deploy Count: {deploy_count}");
                println!(" Shard ID: {shard_id}");
                println!(" Fault Tolerance: {fault_tolerance:.6}");
            } else {
                println!(
                    " Failed to get last finalized block: HTTP {}",
                    response.status()
                );
                println!("Error: {}", response.text().await?);
            }
        }
        Err(e) => {
            println!(" Connection failed!");
            println!("Error: {e}");
            return Err(e.into());
        }
    }

    Ok(())
}

pub async fn show_main_chain_command(
    args: &ShowMainChainArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    println!(
        " Getting main chain blocks from {}:{}",
        args.host, args.port
    );
    println!(" Depth: {} blocks", args.depth);

    // Initialize the F1r3fly API client
    let f1r3fly_api = F1r3flyApi::new_readonly(&args.host, args.port);

    let start_time = Instant::now();

    match f1r3fly_api.show_main_chain(args.depth).await {
        Ok(blocks) => {
            let duration = start_time.elapsed();
            println!(" Main chain blocks retrieved successfully!");
            println!(" Time taken: {duration:.2?}");
            println!(" Found {} blocks in main chain", blocks.len());
            println!();

            if blocks.is_empty() {
                println!(" No blocks found in main chain");
            } else {
                println!(" Main Chain Blocks:");
                for (index, block) in blocks.iter().enumerate() {
                    println!(" Block #{}:", block.block_number);
                    println!(" Hash: {}", block.block_hash);
                    let sender_display = if block.sender.len() >= 16 {
                        format!("{}...", &block.sender[..16])
                    } else if block.sender.is_empty() {
                        "(genesis)".to_string()
                    } else {
                        block.sender.clone()
                    };
                    println!(" Sender: {sender_display}");
                    println!(" Timestamp: {}", block.timestamp);
                    println!(" Deploy Count: {}", block.deploy_count);
                    println!(" Fault Tolerance: {:.6}", block.fault_tolerance);
                    if index < blocks.len() - 1 {
                        println!(" ");
                    }
                }
            }
        }
        Err(e) => {
            println!(" Failed to get main chain blocks!");
            println!("Error: {e}");
            return Err(e);
        }
    }

    Ok(())
}

pub async fn validator_status_command(
    args: &ValidatorStatusArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    println!(" Checking validator status for: {}", args.public_key);

    let f1r3fly_api = F1r3flyApi::new_readonly(&args.host, args.port);

    let start_time = Instant::now();

    let main_chain = f1r3fly_api.show_main_chain(1).await?;
    let current_block = main_chain
        .first()
        .ok_or("No blocks found in main chain")?
        .block_number;
    let snapshot = fetch_pos_snapshot(&args.host, args.http_port).await?;

    println!(" Validator status retrieved successfully!");
    println!(" Time taken: {:.2?}", start_time.elapsed());
    println!();

    let key = &args.public_key;
    match snapshot.stake(key) {
        Some(stake) => {
            println!(" BONDED: stake {stake}");
            if snapshot.is_active(key) {
                println!(" ACTIVE: participating in consensus");
            } else {
                println!(" PENDING ACTIVATION: joins the active set at the next epoch boundary");
            }
        }
        None => println!(" NOT BONDED"),
    }
    if let Some(quarantine_end) = snapshot.pending_withdrawal(key) {
        println!(
            " WITHDRAWAL REQUESTED: leaves the bond set at the next epoch boundary; \
             stake is paid out at the first epoch boundary at or after block {quarantine_end}"
        );
    }
    if let Some(withdrawal) = snapshot.withdrawal(key) {
        println!(
            " WITHDRAWING: stake {} is paid out at the first epoch boundary at or after block {}",
            withdrawal.stake, withdrawal.quarantine_end
        );
    }

    println!();
    println!(" Current Block: {current_block}");

    Ok(())
}

pub async fn epoch_info_command(args: &PosQueryArgs) -> Result<(), Box<dyn std::error::Error>> {
    println!(
        " Getting current epoch information from {}:{}",
        args.host, args.port
    );

    let f1r3fly_api = F1r3flyApi::new_readonly(&args.host, args.port);

    let start_time = Instant::now();

    // Query epoch and quarantine lengths from PoS contract
    let epoch_length_query = r#"new return, rl(`rho:registry:lookup`), poSCh in {
 rl!(`rho:system:pos`, *poSCh) |
 for(@(_, PoS) <- poSCh) {
 @PoS!("getEpochLength", *return)
 }
 }"#;

    let quarantine_length_query = r#"new return, rl(`rho:registry:lookup`), poSCh in {
 rl!(`rho:system:pos`, *poSCh) |
 for(@(_, PoS) <- poSCh) {
 @PoS!("getQuarantineLength", *return)
 }
 }"#;

    // Get main chain tip first to ensure consistent state reference
    let main_chain = f1r3fly_api.show_main_chain(1).await?;
    let tip_block = main_chain.first().ok_or("No blocks found in main chain")?;
    let current_block = tip_block.block_number;
    let tip_block_hash = &tip_block.block_hash;

    // Get epoch and quarantine data using explicit tip block hash for consistency.
    // The node admits only `api-server.exploratory-deploy-max-concurrent` exploratory
    // queries (default 1) and rejects the excess immediately instead of queueing it,
    // so the two exploratory queries run in sequence. `show_main_chain` is not an
    // exploratory query and holds no slot, so it stays parallel.
    let (epoch_result, recent_blocks) = tokio::try_join!(
        f1r3fly_api.exploratory_deploy(epoch_length_query, Some(tip_block_hash), false),
        f1r3fly_api.show_main_chain(5)
    )?;
    let quarantine_result = f1r3fly_api
        .exploratory_deploy(quarantine_length_query, Some(tip_block_hash), false)
        .await?;

    let duration = start_time.elapsed();

    // Parse epoch length from PoS contract result
    let epoch_length = epoch_result.0.trim().parse::<i64>().map_err(|e| {
        format!(
            "Failed to parse epoch length from PoS contract: '{}'. Error: {}",
            epoch_result.0, e
        )
    })?;

    // Parse quarantine length from PoS contract result
    let quarantine_length = quarantine_result.0.trim().parse::<i64>().map_err(|e| {
        format!(
            "Failed to parse quarantine length from PoS contract: '{}'. Error: {}",
            quarantine_result.0, e
        )
    })?;

    // Calculate epoch information
    let current_epoch = current_block / epoch_length;
    let epoch_start_block = current_epoch * epoch_length;
    let epoch_end_block = epoch_start_block + epoch_length - 1;
    let blocks_into_epoch = current_block - epoch_start_block;
    let blocks_remaining = epoch_length - blocks_into_epoch;

    println!(" Epoch information retrieved successfully!");
    println!(" Time taken: {duration:.2?}");
    println!();

    println!(" Current Epoch Status:");
    println!(" Current Block: {current_block}");
    println!(" Current Epoch: {current_epoch}");
    println!(" Epoch Length: {epoch_length} blocks");
    println!(" Quarantine Length: {quarantine_length} blocks");
    println!();

    println!(" Epoch {current_epoch} Details:");
    println!(" Start Block: {epoch_start_block}");
    println!(" End Block: {epoch_end_block}");
    println!(
        " Progress: {}/{} blocks ({:.1}%)",
        blocks_into_epoch,
        epoch_length,
        (blocks_into_epoch as f64 / epoch_length as f64) * 100.0
    );
    println!(" Remaining: {blocks_remaining} blocks");
    println!();

    if blocks_remaining <= 100 {
        println!(" Epoch transition approaching! ({blocks_remaining} blocks remaining)");
    } else if blocks_into_epoch <= 100 {
        println!(" Recently started new epoch! ({blocks_into_epoch} blocks into epoch)");
    }

    println!(" Next Epoch ({}):", current_epoch + 1);
    println!(" Will start at block: {}", epoch_end_block + 1);
    println!(" Estimated blocks until transition: {blocks_remaining}");

    // Show recent block activity
    println!();
    println!(" Recent Block Activity:");
    for block in recent_blocks.iter() {
        let block_epoch = block.block_number / epoch_length;
        let epoch_marker = if block_epoch != current_epoch {
            format!(" (Epoch {block_epoch})")
        } else {
            String::new()
        };

        println!(
            " Block {}:  finalized{}",
            block.block_number, // All main chain blocks are considered finalized
            epoch_marker
        );
    }

    Ok(())
}

pub async fn epoch_rewards_command(args: &PosQueryArgs) -> Result<(), Box<dyn std::error::Error>> {
    println!(
        " Getting current epoch rewards from {}:{}",
        args.host, args.http_port
    );

    let rewards_query = r#"new return, rl(`rho:registry:lookup`), poSCh in {
 rl!(`rho:system:pos`, *poSCh) |
 for(@(_, PoS) <- poSCh) {
 @PoS!("getCurrentEpochRewards", *return)
 }
 }"#;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()?;
    let http_url = format!("http://{}:{}/api/explore-deploy", args.host, args.http_port);

    let start_time = Instant::now();

    let body = serde_json::json!({ "term": rewards_query });
    let response = client.post(&http_url).json(&body).send().await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        println!(" Failed to get epoch rewards!");
        println!("Error: HTTP {status} {body}");
        return Err(format!("HTTP error: {status}").into());
    }

    let response_json: serde_json::Value = response.json().await?;
    let duration = start_time.elapsed();

    println!(" Epoch rewards retrieved successfully!");
    println!(" Time taken: {duration:.2?}");

    // Extract block info
    if let Some(block) = response_json.get("block") {
        let block_hash = block
            .get("blockHash")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");
        let block_number = block
            .get("blockNumber")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        println!(" Block hash: {block_hash}, Block number: {block_number}");
    }

    // Parse rewards from ExprMap: { validator_pubkey: ExprInt { data: reward } }
    println!();
    if let Some(expr) = response_json.get("expr").and_then(|e| e.as_array()) {
        if let Some(expr_map) = expr
            .first()
            .and_then(|e| e.get("ExprMap"))
            .and_then(|m| m.get("data"))
            .and_then(|d| d.as_object())
        {
            println!(" Current Epoch Rewards ({} validators):", expr_map.len());
            println!();

            let mut entries: Vec<(&String, i64)> = expr_map
                .iter()
                .map(|(key, val)| {
                    let reward = val
                        .get("ExprInt")
                        .and_then(|e| e.get("data"))
                        .and_then(|d| d.as_i64())
                        .unwrap_or(0);
                    (key, reward)
                })
                .collect();
            let total_rewards: i64 = entries.iter().map(|(_, r)| r).sum();
            entries.sort_by(|a, b| b.1.cmp(&a.1));

            for (key, reward) in &entries {
                let short_key = if key.len() > 16 {
                    format!("{}...{}", &key[..8], &key[key.len() - 8..])
                } else {
                    key.to_string()
                };
                println!(" {short_key} : {reward}");
            }

            println!();
            println!(" Total: {total_rewards}");
        } else {
            println!(" Current Epoch Rewards:");
            println!("{}", serde_json::to_string_pretty(&response_json["expr"])?);
        }
    } else {
        println!("No reward data returned");
    }

    Ok(())
}

/// Read the PoS contract's validator bookkeeping through a read-only node's HTTP API.
///
/// The node admits only `api-server.exploratory-deploy-max-concurrent` exploratory
/// queries (default 1) and rejects the excess with 503 instead of queueing it, so an
/// occupied slot is retried.
async fn fetch_pos_snapshot(
    host: &str,
    http_port: u16,
) -> Result<PosSnapshot, Box<dyn std::error::Error>> {
    let client = reqwest::Client::new();
    let url = build_url(host, http_port, "/api/explore-deploy");
    let body = serde_json::json!({ "term": POS_SNAPSHOT_QUERY });

    let mut attempt = 0;
    loop {
        let response = client.post(&url).json(&body).send().await?;
        let status = response.status();

        if status.is_success() {
            let json: serde_json::Value = response.json().await?;
            return Ok(parse_pos_snapshot(&json)?);
        }

        if status == reqwest::StatusCode::SERVICE_UNAVAILABLE
            && attempt < EXPLORATORY_RETRY_BACKOFF.len()
        {
            tokio::time::sleep(EXPLORATORY_RETRY_BACKOFF[attempt]).await;
            attempt += 1;
            continue;
        }

        return Err(format!("HTTP {status}: {}", response.text().await?).into());
    }
}

fn abbreviate_key(key: &str) -> String {
    if key.len() > 16 {
        format!("{}...{}", &key[..8], &key[key.len() - 8..])
    } else {
        key.to_string()
    }
}

pub async fn network_consensus_command(
    args: &PosQueryArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    println!(
        " Getting network-wide consensus overview from {}:{}",
        args.host, args.port
    );

    let f1r3fly_api = F1r3flyApi::new_readonly(&args.host, args.port);

    let start_time = Instant::now();

    let quarantine_query = r#"new return, rl(`rho:registry:lookup`), poSCh in {
 rl!(`rho:system:pos`, *poSCh) |
 for(@(_, PoS) <- poSCh) {
 @PoS!("getQuarantineLength", *return)
 }
 }"#;

    // Get main chain tip first to ensure consistent state reference
    let main_chain = f1r3fly_api.show_main_chain(1).await?;
    let tip_block = main_chain.first().ok_or("No blocks found in main chain")?;
    let current_block = tip_block.block_number;
    let tip_block_hash = &tip_block.block_hash;

    // Both reads are exploratory queries, which the node admits only
    // `api-server.exploratory-deploy-max-concurrent` of at a time (default 1) and
    // rejects rather than queues, so they run in sequence.
    let snapshot = fetch_pos_snapshot(&args.host, args.http_port).await?;
    let quarantine_result = f1r3fly_api
        .exploratory_deploy(quarantine_query, Some(tip_block_hash), false)
        .await?;

    let duration = start_time.elapsed();

    println!(" Network consensus data retrieved successfully!");
    println!(" Time taken: {duration:.2?}");
    println!();

    let quarantine_length = quarantine_result.0.trim().parse::<i64>().map_err(|e| {
        format!(
            "Failed to parse quarantine length: '{}'. Error: {}",
            quarantine_result.0, e
        )
    })?;

    let total_bonded = snapshot.bonds.len();
    let total_active = snapshot.active.len();

    println!(" Network Consensus Health:");
    println!(" Current Block: {current_block}");
    println!(" Total Bonded Validators: {total_bonded}");
    println!(" Active Validators: {total_active}");
    println!(
        " Pending Activation: {}",
        snapshot.pending_activation().count()
    );
    println!(
        " Pending Withdrawals: {}",
        snapshot.pending_withdrawals.len()
    );
    println!(" Withdrawing: {}", snapshot.withdrawals.len());
    println!(" Withdrawal Quarantine Length: {quarantine_length} blocks");

    let consensus_health = if total_active >= 3 {
        " Healthy"
    } else if total_active >= 1 {
        " Limited"
    } else {
        " Critical"
    };

    println!(" Consensus Status: {consensus_health}");

    if total_bonded > 0 {
        let participation_rate = (total_active as f64 / total_bonded as f64) * 100.0;
        println!(" Participation Rate: {participation_rate:.1}%");
    }

    Ok(())
}

pub async fn get_blocks_by_height_command(
    args: &GetBlocksByHeightArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    println!(
        " Getting blocks by height range from {}:{}",
        args.host, args.port
    );
    println!(
        " Block range: {} to {}",
        args.start_block_number, args.end_block_number
    );

    // Validate block range
    if args.start_block_number > args.end_block_number {
        return Err("Start block number must be less than or equal to end block number".into());
    }

    if args.start_block_number < 0 || args.end_block_number < 0 {
        return Err("Block numbers must be non-negative".into());
    }

    // Initialize the F1r3fly API client
    let f1r3fly_api = F1r3flyApi::new_readonly(&args.host, args.port);

    let start_time = Instant::now();

    match f1r3fly_api
        .get_blocks_by_height(args.start_block_number, args.end_block_number)
        .await
    {
        Ok(blocks) => {
            let duration = start_time.elapsed();
            println!(" Blocks retrieved successfully!");
            println!(" Time taken: {duration:.2?}");
            println!(" Found {} blocks in height range", blocks.len());
            println!();

            if blocks.is_empty() {
                println!(" No blocks found in the specified height range");
            } else {
                println!(" Blocks by Height:");
                for (index, block) in blocks.iter().enumerate() {
                    println!(" Block #{}:", block.block_number);
                    println!(" Hash: {}", block.block_hash);
                    let sender_display = if block.sender.len() >= 16 {
                        format!("{}...", &block.sender[..16])
                    } else if block.sender.is_empty() {
                        "(genesis)".to_string()
                    } else {
                        block.sender.clone()
                    };
                    println!(" Sender: {sender_display}");
                    println!(" Timestamp: {}", block.timestamp);
                    println!(" Deploy Count: {}", block.deploy_count);
                    println!(" Fault Tolerance: {:.6}", block.fault_tolerance);
                    if index < blocks.len() - 1 {
                        println!(" ");
                    }
                }
            }
        }
        Err(e) => {
            println!(" Failed to get blocks by height!");
            println!("Error: {e}");
            return Err(e);
        }
    }

    Ok(())
}

/// Validates that when using -H with a remote host, --custom-ports must be specified
fn validate_host_and_ports(host: &str, custom_ports: &Option<String>) -> Result<(), String> {
    match (host, custom_ports) {
        // Remote host without custom ports - ERROR
        (h, None) if h != "localhost" && h != "127.0.0.1" => Err(format!(
            "When using -H with remote host '{h}', you must specify --custom-ports\n\
 \n\
 Remote hosts don't use standard F1r3fly ports. Specify the actual ports:\n\
 \n\
 Examples:\n\
 cargo run -- network-health -H {h} --custom-ports \"8001,8002,9443\"\n\
 cargo run -- network-health -H {h} --custom-ports \"7890\"\n\
 \n\
 For localhost, standard ports are assumed:\n\
 cargo run -- network-health -H localhost (uses standard ports)\n\
 cargo run -- network-health (uses localhost + standard ports)"
        )),
        // All other combinations are valid
        _ => Ok(()),
    }
}

pub async fn block_transfers_command(
    args: &BlockTransfersArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Getting transfers from block: {}", args.block_hash);

    let url = format!(
        "http://{}:{}/api/block/{}",
        args.host, args.port, args.block_hash
    );
    let client = reqwest::Client::new();
    let start_time = Instant::now();

    let response = client.get(&url).send().await?;
    let duration = start_time.elapsed();

    if !response.status().is_success() {
        println!("Failed to get block: HTTP {}", response.status());
        return Err(format!("HTTP {}", response.status()).into());
    }

    let block_json: serde_json::Value = response.json().await?;

    println!("Block retrieved successfully!");
    println!("Time taken: {duration:.2?}");
    println!();

    // Extract block info
    let block_number = block_json
        .get("blockInfo")
        .and_then(|b| b.get("blockNumber"))
        .and_then(|n| n.as_i64())
        .unwrap_or(0);

    let block_hash_display = if args.block_hash.len() > 16 {
        format!("{}...", &args.block_hash[..16])
    } else {
        args.block_hash.clone()
    };

    println!("Block #{block_number} ({block_hash_display})");
    println!();

    // Extract deploys and their transfers
    let deploys = block_json
        .get("deploys")
        .and_then(|d| d.as_array())
        .map(|a| a.to_vec())
        .unwrap_or_default();

    let mut total_transfers = 0;
    let mut deploys_with_transfers = 0;
    let mut successful_transfers = 0;
    let mut failed_transfers = 0;

    for (i, deploy) in deploys.iter().enumerate() {
        let sig = deploy
            .get("sig")
            .and_then(|s| s.as_str())
            .unwrap_or("unknown");

        let transfers = deploy
            .get("transfers")
            .and_then(|t| t.as_array())
            .map(|a| a.to_vec())
            .unwrap_or_default();

        if transfers.is_empty() && !args.all_deploys {
            continue;
        }

        if !transfers.is_empty() {
            deploys_with_transfers += 1;
        }

        let sig_display = if sig.len() > 20 {
            format!("{}...", &sig[..20])
        } else {
            sig.to_string()
        };

        println!("Deploy #{} (sig: {})", i + 1, sig_display);

        if transfers.is_empty() {
            println!(" No transfers");
        } else {
            for (j, transfer) in transfers.iter().enumerate() {
                total_transfers += 1;
                let from = transfer
                    .get("fromAddr")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                let to = transfer
                    .get("toAddr")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?");
                let amount = transfer.get("amount").and_then(|v| v.as_i64()).unwrap_or(0);
                let success = transfer
                    .get("success")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let fail_reason = transfer
                    .get("failReason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                if success {
                    successful_transfers += 1;
                } else {
                    failed_transfers += 1;
                }

                let status = if success {
                    "Success".to_string()
                } else {
                    format!("Failed: {fail_reason}")
                };

                println!(" Transfer #{}:", j + 1);
                println!(" From: {from}");
                println!(" To: {to}");
                println!(" Amount: {amount}");
                println!(" Status: {status}");
            }
        }
        println!();
    }

    // Summary
    println!("Summary:");
    println!(" Total deploys in block: {}", deploys.len());
    println!(" Deploys with transfers: {deploys_with_transfers}");
    println!(" Total transfers: {total_transfers}");
    if total_transfers > 0 {
        println!(" Successful: {successful_transfers}");
        println!(" Failed: {failed_transfers}");
    }

    Ok(())
}

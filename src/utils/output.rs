use serde_json;
use std::time::Duration;

// Emoji constants
pub const EMOJI_SEARCH: &str = "";
pub const EMOJI_SUCCESS: &str = "";
pub const EMOJI_ERROR: &str = "";
pub const EMOJI_INFO: &str = "";
pub const EMOJI_TIME: &str = "";
pub const EMOJI_ROCKET: &str = "";
pub const EMOJI_FILE: &str = "";
pub const EMOJI_BLOCK: &str = "";
pub const EMOJI_CONNECT: &str = "";
pub const EMOJI_KEY: &str = "";
pub const EMOJI_LINK: &str = "";
pub const EMOJI_WARNING: &str = "";

// Output formatting functions
pub fn print_info(message: &str) {
    println!("{EMOJI_INFO} {message}");
}

pub fn print_success(message: &str) {
    println!("{EMOJI_SUCCESS} {message}");
}

pub fn print_error(message: &str) {
    println!("{EMOJI_ERROR} {message}");
}

pub fn print_search(message: &str) {
    println!("{EMOJI_SEARCH} {message}");
}

pub fn print_time(message: &str, duration: Duration) {
    println!("{EMOJI_TIME} {message}: {duration:.2?}");
}

pub fn print_file_info(filename: &str, size: usize) {
    println!("{EMOJI_FILE} Reading Rholang from: {filename}");
    println!("{EMOJI_INFO} Code size: {size} bytes");
}

pub fn print_connection(host: &str, port: u16) {
    println!("{EMOJI_CONNECT} Connecting to F1r3fly node at {host}:{port}");
}

pub fn print_block_info(block_hash: &str) {
    println!("{EMOJI_BLOCK} Block hash: {block_hash}");
}

pub fn print_rocket(message: &str) {
    println!("{EMOJI_ROCKET} {message}");
}

pub fn print_key(key_type: &str, key_value: &str) {
    println!("{EMOJI_KEY} {key_type}: {key_value}");
}

pub fn print_json_pretty(
    title: &str,
    json: &serde_json::Value,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("{EMOJI_INFO} {title}:");
    println!("{}", serde_json::to_string_pretty(json)?);
    Ok(())
}

pub fn print_warning(message: &str) {
    println!("{EMOJI_WARNING} {message}");
}

pub fn print_bond_status(is_bonded: bool) {
    if is_bonded {
        println!("{EMOJI_LINK} {EMOJI_SUCCESS} Validator is BONDED");
    } else {
        println!("{EMOJI_LINK} {EMOJI_ERROR} Validator is NOT BONDED");
    }
}

pub fn print_health_status(healthy: u32, total: u32) {
    println!("{EMOJI_SUCCESS} Healthy nodes: {healthy}/{total}");
}

pub fn print_network_status(healthy: u32, total: u32) {
    if healthy == 0 {
        print_error("No healthy nodes found - check if network is running");
    } else if healthy == total {
        print_warning("All nodes healthy but some peer connections may be missing");
    } else {
        print_warning("Some nodes are unhealthy - check individual node logs");
    }
}

// Helper function to format operation results
pub fn format_operation_result(success: bool, operation: &str, duration: Duration) {
    if success {
        print_success(&format!("{operation} successful!"));
    } else {
        print_error(&format!("{operation} failed!"));
    }
    print_time("Time taken", duration);
}

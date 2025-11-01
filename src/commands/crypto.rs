use crate::args::*;
use crate::error::{NodeCliError, Result};
use crate::utils::{print_key, print_success, CryptoUtils};
use std::fs;
use std::path::Path;

pub fn generate_public_key_command(args: &GeneratePublicKeyArgs) -> Result<()> {
    // Decode private key using crypto utils
    let secret_key = CryptoUtils::decode_private_key(&args.private_key)?;

    // Derive public key from private key
    let public_key = CryptoUtils::derive_public_key(&secret_key);

    // Serialize public key in the requested format
    let public_key_hex = CryptoUtils::serialize_public_key(&public_key, args.compressed);

    // Print the public key using output utils
    let key_type = if args.compressed {
        "compressed"
    } else {
        "uncompressed"
    };
    print_key(&format!("Public key ({})", key_type), &public_key_hex);

    Ok(())
}

pub fn generate_key_pair_command(args: &GenerateKeyPairArgs) -> Result<()> {
    // Generate a new random key pair
    let (secret_key, public_key) = CryptoUtils::generate_key_pair()?;

    // Serialize keys
    let private_key_hex = CryptoUtils::serialize_private_key(&secret_key);
    let public_key_hex = CryptoUtils::serialize_public_key(&public_key, args.compressed);

    if args.save {
        // Create output directory if it doesn't exist
        let output_dir = Path::new(&args.output_dir);
        if !output_dir.exists() {
            fs::create_dir_all(output_dir).map_err(|e| {
                NodeCliError::file_write_failed(
                    &output_dir.display().to_string(),
                    &format!("Failed to create directory: {}", e),
                )
            })?;
        }

        // Create filenames
        let private_key_file = output_dir.join("private_key.hex");
        let public_key_file = output_dir.join("public_key.hex");

        // Write keys to files
        fs::write(&private_key_file, &private_key_hex).map_err(|e| {
            NodeCliError::file_write_failed(&private_key_file.display().to_string(), &e.to_string())
        })?;

        fs::write(&public_key_file, &public_key_hex).map_err(|e| {
            NodeCliError::file_write_failed(&public_key_file.display().to_string(), &e.to_string())
        })?;

        print_success(&format!(
            "Private key saved to: {}",
            private_key_file.display()
        ));
        print_success(&format!(
            "Public key saved to: {}",
            public_key_file.display()
        ));
    } else {
        // Print the keys using output utils
        print_key("Private key", &private_key_hex);
        let key_type = if args.compressed {
            "compressed"
        } else {
            "uncompressed"
        };
        print_key(&format!("Public key ({})", key_type), &public_key_hex);
    }

    Ok(())
}

pub fn generate_address_command(args: &GenerateAddressArgs) -> Result<()> {
    // Determine the public key to use
    let public_key_hex = if let Some(public_key_hex) = &args.public_key {
        // Use provided public key
        public_key_hex.clone()
    } else if let Some(private_key_hex) = &args.private_key {
        // Derive public key from private key
        let secret_key = CryptoUtils::decode_private_key(private_key_hex)?;
        let public_key = CryptoUtils::derive_public_key(&secret_key);
        // Use uncompressed format for address generation
        CryptoUtils::serialize_public_key(&public_key, false)
    } else {
        return Err(NodeCliError::config_missing_required(
            "Either --public-key or --private-key must be provided",
        ));
    };

    // Validate the public key
    if !CryptoUtils::is_valid_public_key(&public_key_hex) {
        return Err(NodeCliError::crypto_invalid_public_key(
            "Invalid public key format",
        ));
    }

    // Generate address
    let address = CryptoUtils::generate_address(&public_key_hex)?;

    // Print the result using output utils
    print_key("Public key", &public_key_hex);
    print_key("Address", &address);

    Ok(())
}

pub fn get_node_id_command(args: &GetNodeIdArgs) -> Result<()> {
    use sha3::Digest;
    use std::process::Command;

    println!("🔑 Extracting node ID from TLS key file: {}", args.key_file);

    // Use OpenSSL command following F1R3FLY's documented approach
    let output = Command::new("openssl")
        .args(&["ec", "-text", "-in", &args.key_file, "-noout"])
        .output()
        .map_err(|e| {
            NodeCliError::crypto_invalid_private_key(&format!("Failed to execute openssl: {}", e))
        })?;

    if !output.status.success() {
        let error_msg = String::from_utf8_lossy(&output.stderr);
        return Err(NodeCliError::crypto_invalid_private_key(&format!(
            "OpenSSL error: {}",
            error_msg
        )));
    }

    let openssl_output = String::from_utf8_lossy(&output.stdout);

    // Debug: Uncomment to see OpenSSL output
    // println!("🔍 Debug: OpenSSL output:");
    // println!("{}", openssl_output);

    // Extract public key from OpenSSL output
    let public_key_hex = extract_public_key_from_openssl_output(&openssl_output)?;

    // Remove the '04' prefix as per F1R3FLY specification
    let cleaned_hex = if public_key_hex.starts_with("04") {
        &public_key_hex[2..]
    } else {
        &public_key_hex
    };

    // Convert hex to bytes
    let public_key_bytes = hex::decode(cleaned_hex)
        .map_err(|e| NodeCliError::crypto_invalid_private_key(&format!("Invalid hex: {}", e)))?;

    // Calculate Keccac-256 hash
    let mut hasher = sha3::Keccak256::new();
    hasher.update(&public_key_bytes);
    let hash = hasher.finalize();

    // Take last 20 bytes (40 hex characters) for node ID
    let node_id = hex::encode(&hash[hash.len() - 20..]);

    // Output based on format
    match args.format.as_str() {
        "hex" => {
            print_success("Node ID extracted successfully!");
            print_key("Node ID", &node_id);
        }
        "rnode-url" => {
            let rnode_url = format!(
                "rnode://{}@{}?protocol={}&discovery={}",
                node_id, args.host, args.protocol_port, args.discovery_port
            );
            print_success("Node ID extracted successfully!");
            print_key("Node ID", &node_id);
            print_key("RNode URL", &rnode_url);
        }
        _ => {
            return Err(NodeCliError::General(
                "Invalid format. Use 'hex' or 'rnode-url'".to_string(),
            ));
        }
    }

    Ok(())
}

fn extract_public_key_from_openssl_output(output: &str) -> Result<String> {
    let mut in_pub_section = false;
    let mut public_key_hex = String::new();

    for line in output.lines() {
        let trimmed = line.trim();

        if trimmed == "pub:" {
            in_pub_section = true;
            continue;
        }

        // Stop when we hit the next section (lines like "ASN1 OID:" or other non-hex lines)
        if in_pub_section
            && (trimmed.contains("ASN1") || trimmed.contains("NIST") || trimmed.contains("OID"))
        {
            break;
        }

        if in_pub_section {
            // Extract hex bytes from lines like "    04:00:81:19:bf:90:eb:01:09:a0:ea:67:9f:df:5e:"
            // Split by colon and process each part
            let parts: Vec<&str> = trimmed.split(':').collect();
            for part in parts {
                let hex_part = part.trim();
                // Check if it's a 2-character hex string
                if hex_part.len() == 2 && hex_part.chars().all(|c| c.is_ascii_hexdigit()) {
                    public_key_hex.push_str(hex_part);
                }
            }
        }
    }

    if public_key_hex.is_empty() {
        return Err(NodeCliError::crypto_invalid_private_key(
            "Could not extract public key from OpenSSL output",
        ));
    }

    // Debug: Uncomment to see extracted hex
    // println!("🔍 Debug: Final extracted public key hex: {}", public_key_hex);
    Ok(public_key_hex)
}

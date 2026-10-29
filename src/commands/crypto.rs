use crate::args::*;
use crate::commands::CliResult;
use crate::utils::{print_key, print_success, CryptoUtils};
use std::fs;
use std::path::Path;

pub fn generate_public_key_command(args: &GeneratePublicKeyArgs) -> CliResult {
    let public_key = args.key.load()?.public_key();

    // Serialize public key in the requested format
    let public_key_hex = CryptoUtils::serialize_public_key(&public_key, args.compressed);

    // Print the public key using output utils
    let key_type = if args.compressed {
        "compressed"
    } else {
        "uncompressed"
    };
    print_key(&format!("Public key ({key_type})"), &public_key_hex);

    Ok(())
}

pub fn generate_key_pair_command(args: &GenerateKeyPairArgs) -> CliResult {
    let (secret_key, public_key) = CryptoUtils::generate_key_pair();

    if args.save {
        let output_dir = Path::new(&args.output_dir);
        fs::create_dir_all(output_dir).map_err(|cause| crate::Error::Io {
            path: output_dir.to_path_buf(),
            cause,
        })?;

        let private_key_file = output_dir.join("private_key.hex");
        let public_key_file = output_dir.join("public_key.hex");
        CryptoUtils::write_key_pair_to_files(
            &secret_key,
            &public_key,
            &private_key_file,
            &public_key_file,
            args.compressed,
        )?;

        print_success(&format!(
            "Private key saved to: {}",
            private_key_file.display()
        ));
        print_success(&format!(
            "Public key saved to: {}",
            public_key_file.display()
        ));
    } else {
        print_key(
            "Private key",
            &CryptoUtils::serialize_private_key(&secret_key),
        );
        let key_type = if args.compressed {
            "compressed"
        } else {
            "uncompressed"
        };
        print_key(
            &format!("Public key ({key_type})"),
            &CryptoUtils::serialize_public_key(&public_key, args.compressed),
        );
    }

    Ok(())
}

pub fn generate_vault_address_command(args: &GenerateVaultAddressArgs) -> CliResult {
    // Determine the public key to use
    let public_key_hex = match &args.public_key {
        Some(public_key_hex) => public_key_hex.clone(),
        None => match args.key.load_optional()? {
            Some(key) => CryptoUtils::serialize_public_key(&key.public_key(), false),
            None => {
                return Err(
                    "pass --public-key, or a signing key with --private-key-file or FIREFLY_PRIVATE_KEY"
                        .into(),
                )
            }
        },
    };

    if !CryptoUtils::is_valid_public_key(&public_key_hex) {
        return Err(crate::Error::invalid("public key", "not a 33- or 65-byte hex key").into());
    }

    // Generate vault address
    let vault_address = CryptoUtils::generate_vault_address(&public_key_hex)?;

    // Print the result using output utils
    print_key("Public key", &public_key_hex);
    print_key("Vault address", &vault_address);

    Ok(())
}

pub fn get_node_id_command(args: &GetNodeIdArgs) -> CliResult {
    use sha3::Digest;
    use std::process::Command;

    // Determine which file to use
    let (file_path, file_type) = if let Some(key_file) = &args.key_file {
        (key_file.as_str(), "TLS private key")
    } else if let Some(cert_file) = &args.cert_file {
        (cert_file.as_str(), "TLS certificate")
    } else {
        return Err("either --key-file or --cert-file must be provided".into());
    };

    println!(" Extracting node ID from {file_type} file: {file_path}");

    // Use appropriate OpenSSL command based on file type
    let openssl_args: &[&str] = if args.key_file.is_some() {
        &["ec", "-text", "-in", file_path, "-noout"]
    } else {
        &["x509", "-in", file_path, "-noout", "-text"]
    };
    let output = Command::new("openssl")
        .args(openssl_args)
        .output()
        .map_err(|e| format!("could not run openssl: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "openssl could not read {file_path}: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
        .into());
    }
    let openssl_output = String::from_utf8_lossy(&output.stdout).to_string();

    // Debug: Uncomment to see OpenSSL output
    // println!(" Debug: OpenSSL output:");
    // println!("{}", openssl_output);

    // Extract public key from OpenSSL output
    let public_key_hex = extract_public_key_from_openssl_output(&openssl_output)?;

    // Remove the '04' prefix as per F1R3FLY specification
    let cleaned_hex = public_key_hex.strip_prefix("04").unwrap_or(&public_key_hex);

    // Convert hex to bytes
    let public_key_bytes = hex::decode(cleaned_hex)
        .map_err(|e| format!("openssl printed a public key that is not hex: {e}"))?;

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
        other => {
            return Err(format!("invalid format {other:?}; use 'hex' or 'rnode-url'").into());
        }
    }

    Ok(())
}

fn extract_public_key_from_openssl_output(output: &str) -> CliResult<String> {
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
            // Extract hex bytes from lines like " 04:00:81:19:bf:90:eb:01:09:a0:ea:67:9f:df:5e:"
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
        return Err("openssl printed no public key".into());
    }

    // Debug: Uncomment to see extracted hex
    // println!(" Debug: Final extracted public key hex: {}", public_key_hex);
    Ok(public_key_hex)
}

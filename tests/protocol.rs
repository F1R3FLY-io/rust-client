//! Compatibility with the node's canonical monetary and signing interfaces.
use f1r3fly_crypto::rust::{private_key::PrivateKey, signatures::secp256k1::Secp256k1};
use f1r3fly_models::casper::DeployDataProto;
use f1r3fly_models::rust::casper::protocol::casper_message::DeployData;
use f1r3fly_shared::rust::token_amount::TokenAmount;
use node_cli::f1r3fly_api::{DeployDetail, NodeStatus};
use node_cli::{DeployOptions, RhoValue, SignedDeploy, SigningKey, UnsignedDeploy};
use num_bigint::BigUint;
use prost::Message;
use serde_json::json;

#[test]
fn status_and_deploy_prices_are_uint256_and_strictly_parsed() {
    let mut status = json!({"version": {}, "address": "node", "networkId": "test",
        "shardId": "root", "peers": 0, "nodes": 0, "minPhloPrice": "1"});
    let mut detail = json!({"deployId": "signature", "blockHash": "ab", "blockNumber": 1, "timestamp": 1,
        "cost": 1, "errored": false, "isFinalized": true, "phloPrice": "1"});
    for price in [
        BigUint::from(0u8),
        BigUint::from(1u8),
        BigUint::from(1u8) << 128usize,
        (BigUint::from(1u8) << 256usize) - 1u8,
    ] {
        status["minPhloPrice"] = json!(price.to_string());
        let parsed: NodeStatus = serde_json::from_value(status.clone()).unwrap();
        assert_eq!(parsed.min_phlo_price, price);
        assert_eq!(
            serde_json::to_value(parsed).unwrap()["minPhloPrice"],
            status["minPhloPrice"]
        );
        detail["phloPrice"] = json!(price.to_string());
        let parsed: DeployDetail = serde_json::from_value(detail.clone()).unwrap();
        assert_eq!(parsed.phlo_price, Some(price));
        assert_eq!(
            serde_json::to_value(parsed).unwrap()["phloPrice"],
            detail["phloPrice"]
        );
    }
    for invalid in [
        json!(-1),
        json!("-1"),
        json!("+1"),
        json!("01"),
        json!("-0"),
        json!("1.0"),
        json!("1e2"),
        json!(" 1"),
        json!("1 "),
        json!(""),
        json!((BigUint::from(1u8) << 256usize).to_string()),
    ] {
        status["minPhloPrice"] = invalid.clone();
        detail["phloPrice"] = invalid.clone();
        assert!(
            serde_json::from_value::<NodeStatus>(status.clone()).is_err(),
            "{invalid}"
        );
        assert!(
            serde_json::from_value::<DeployDetail>(detail.clone()).is_err(),
            "{invalid}"
        );
    }
    detail.as_object_mut().unwrap().remove("phloPrice");
    assert_eq!(
        serde_json::from_value::<DeployDetail>(detail.clone())
            .unwrap()
            .phlo_price,
        None
    );
    detail["phloPrice"] = serde_json::Value::Null;
    assert_eq!(
        serde_json::from_value::<DeployDetail>(detail)
            .unwrap()
            .phlo_price,
        None
    );
}

#[test]
fn nested_integer_queries_follow_the_new_json_schema() {
    let value = RhoValue::from_node_json(&json!({"ExprList": {"data": [
        {"ExprInt": {"data": i64::MIN.to_string()}},
        {"ExprTuple": {"data": [{"ExprUint64": {"data": u64::MAX.to_string()}},
            {"ExprUint32": {"data": u32::MAX}}]}}
    ]}}))
    .unwrap();
    assert_eq!(value.elements().unwrap()[0], RhoValue::Int(i64::MIN));
    assert_eq!(
        value.elements().unwrap()[1].elements().unwrap()[0],
        RhoValue::Uint64(u64::MAX)
    );
    for (tag, raw) in [
        ("ExprInt", json!("9223372036854775808")),
        ("ExprInt", json!("-9223372036854775809")),
        ("ExprUint64", json!("18446744073709551616")),
        ("ExprUint64", json!("-1")),
        ("ExprInt", json!("01")),
        ("ExprInt", json!("-0")),
        ("ExprUint64", json!("+1")),
    ] {
        assert!(RhoValue::from_node_json(&json!({tag: {"data": raw}})).is_err());
    }
}

#[test]
fn node_encoder_and_verifier_agree_with_client_signatures() {
    use f1r3fly_crypto::rust::signatures::signed::Signed;
    let key = SigningKey::from_hex(&"42".repeat(32)).unwrap();
    let prices = [
        BigUint::from(1u8),
        BigUint::from(i64::MAX as u64),
        BigUint::from(1u8) << 63usize,
        BigUint::from(u64::MAX),
        BigUint::from(1u8) << 128usize,
        (BigUint::from(1u8) << 256usize) - 1u8,
    ];
    // -1 is the anchor a client signs when the node's tip is height 0.
    for (price, valid_after_block_number) in prices
        .iter()
        .flat_map(|price| [-1i64, 7].map(|anchor| (price.clone(), anchor)))
    {
        let options = DeployOptions {
            timestamp: Some(1000),
            phlo_limit: 1,
            phlo_price: price.clone(),
            ..Default::default()
        };
        let client = UnsignedDeploy::new("Nil", &options, valid_after_block_number, "root")
            .unwrap()
            .sign(key.secret_key());
        let proto = DeployDataProto::decode(client.to_bytes().as_slice()).unwrap();
        let verified = DeployData::from_proto(proto).expect("node verifies client signature");
        assert_eq!(verified.data.phlo_price.to_string(), price.to_string());
        // The node signs and encodes independently, using its own projection.
        let node = Signed::create(
            verified.data,
            Box::new(Secp256k1),
            PrivateKey::from_bytes(&[0x42; 32]),
        )
        .unwrap();
        let encoded = DeployData::to_proto_ref(&node).encode_to_vec();
        let imported = SignedDeploy::from_bytes(&encoded).unwrap();
        assert_eq!(imported.id(), client.id());
        assert_eq!(
            imported.to_bytes(),
            encoded,
            "import must preserve the saved bytes"
        );
    }
}

/// Golden vectors shared with the wallet SDK
/// (asi-alliance/asi-chain-wallet-sdk, `tests/fixtures/uint256/node-vectors.json`),
/// generated from the node's crypto and prost code. The file is a byte-identical
/// copy, so both clients are checked against the same expected bytes.
#[test]
fn signing_matches_the_vectors_shared_with_the_wallet_sdk() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/sdk-uint256-node-vectors.json")).unwrap();
    let key = SigningKey::from_hex(&"01".repeat(32)).unwrap();
    let vectors = fixture["vectors"].as_array().unwrap();
    assert_eq!(vectors.len(), 10);
    for vector in vectors {
        let data = &vector["data"];
        let text = |field: &str| vector[field].as_str().unwrap();
        let options = DeployOptions {
            timestamp: Some(data["timestamp"].as_i64().unwrap()),
            phlo_limit: data["phloLimit"].as_u64().unwrap(),
            phlo_price: data["phloPrice"].as_str().unwrap().parse().unwrap(),
            ..Default::default()
        };
        let unsigned = UnsignedDeploy::new(
            data["term"].as_str().unwrap(),
            &options,
            data["validAfterBlockNumber"].as_i64().unwrap(),
            data["shardId"].as_str().unwrap(),
        )
        .unwrap();
        assert_eq!(hex::encode(unsigned.digest()), text("hash"), "{data}");
        let signed = unsigned.sign(key.secret_key());
        assert_eq!(signed.id(), text("signature"), "{data}");

        let mut proto = DeployDataProto::decode(signed.to_bytes().as_slice()).unwrap();
        assert_eq!(hex::encode(&proto.deployer), text("publicKey"), "{data}");
        // The signed projection: the fields a signature covers.
        proto.sig = Default::default();
        proto.deployer = Default::default();
        proto.language = String::new();
        proto.sig_algorithm = String::new();
        assert_eq!(
            hex::encode(proto.encode_to_vec()),
            text("protobufHex"),
            "{data}"
        );

        // The vector's own bytes, signature and key pass both verifiers.
        let mut stored =
            DeployDataProto::decode(hex::decode(text("protobufHex")).unwrap().as_slice()).unwrap();
        stored.sig = hex::decode(text("signature")).unwrap().into();
        stored.deployer = hex::decode(text("publicKey")).unwrap().into();
        stored.sig_algorithm = "secp256k1".into();
        stored.language = "rholang".into();
        SignedDeploy::from_bytes(&stored.encode_to_vec()).expect("client verifies the vector");
        DeployData::from_proto(stored).expect("node verifies the vector");
    }
}

#[test]
fn invalid_prices_and_maximum_charges_fail_before_signing_or_import() {
    let try_new = |price, limit| {
        UnsignedDeploy::new(
            "Nil",
            &DeployOptions {
                phlo_price: price,
                phlo_limit: limit,
                ..Default::default()
            },
            0,
            "root",
        )
    };
    let max = (BigUint::from(1u8) << 256usize) - 1u8;
    assert!(try_new(0u8.into(), 1).is_err());
    assert!(try_new(&max + 1u8, 1).is_err());
    assert!(try_new(max.clone(), 2).is_err());
    let key = SigningKey::from_hex(&"42".repeat(32)).unwrap();
    let signed = try_new(1u8.into(), 1).unwrap().sign(key.secret_key());
    let proto = DeployDataProto::decode(signed.to_bytes().as_slice()).unwrap();
    for length in [0, 1, 31, 33] {
        let mut bad = proto.clone();
        bad.phlo_price = vec![1; length].into();
        assert!(SignedDeploy::from_bytes(&bad.encode_to_vec())
            .unwrap_err()
            .to_string()
            .contains("32 bytes"));
    }
    let mut bad = proto.clone();
    bad.phlo_price = TokenAmount::zero().to_wire().to_vec().into();
    assert!(SignedDeploy::from_bytes(&bad.encode_to_vec())
        .unwrap_err()
        .to_string()
        .contains("positive"));
    bad.phlo_price = TokenAmount::max().to_wire().to_vec().into();
    bad.phlo_limit = 2;
    assert!(SignedDeploy::from_bytes(&bad.encode_to_vec())
        .unwrap_err()
        .to_string()
        .contains("maximum phlo charge"));
    bad.phlo_limit = -1;
    assert!(SignedDeploy::from_bytes(&bad.encode_to_vec()).is_err());
}

#[cfg(feature = "cli")]
#[test]
fn cli_prices_reject_noncanonical_or_out_of_range_inputs() {
    use clap::Parser;
    use node_cli::args::DeployOptionsArgs;
    use node_cli::f1r3fly_api::DeployParameters;
    #[derive(Parser)]
    struct Flags {
        #[command(flatten)]
        options: DeployOptionsArgs,
    }
    assert_eq!(
        Flags::try_parse_from(["price"]).unwrap().options.phlo_price,
        None
    );
    let parameters = DeployParameters {
        endpoint: "http://localhost:40401".to_string(),
        min_phlo_price: None,
        max_phlo_limit: None,
        native_token_decimals: 18,
    };
    let wide = (BigUint::from(1u8) << 128usize).to_string();
    let options = Flags::try_parse_from(["price", "--phlo-price", &wide])
        .unwrap()
        .options
        .options(50_000, &parameters)
        .unwrap();
    assert_eq!(options.phlo_price.to_string(), wide);
    for value in [
        "0",
        "-1",
        "+1",
        "01",
        "1.5",
        "1e18",
        "115792089237316195423570985008687907853269984665640564039457584007913129639936",
    ] {
        assert!(
            Flags::try_parse_from(["price", "--phlo-price", value]).is_err(),
            "{value}"
        );
    }
}

#[cfg(feature = "cli")]
fn parameters(
    min_phlo_price: Option<&str>,
    max_phlo_limit: Option<u64>,
) -> node_cli::f1r3fly_api::DeployParameters {
    node_cli::f1r3fly_api::DeployParameters {
        endpoint: "http://localhost:40401".to_string(),
        min_phlo_price: min_phlo_price.map(|price| price.parse().unwrap()),
        max_phlo_limit,
        native_token_decimals: 18,
    }
}

#[cfg(feature = "cli")]
#[test]
fn cli_prices_and_limits_resolve_from_one_node_status() {
    use clap::Parser;
    use node_cli::args::DeployArgs;

    let args = DeployArgs::try_parse_from(["deploy", "-f", "contract.rho"]).unwrap();
    let resolve = |parameters: &node_cli::f1r3fly_api::DeployParameters, requested| {
        node_cli::args::DeployOptionsArgs::capped_limit(parameters, requested, 50_000)
    };
    let reported = parameters(Some("18446744073709551617"), Some(30_000));
    let limit = resolve(&reported, args.phlo_limit).unwrap();
    let options = args.deploy.options(limit, &reported).unwrap();
    assert_eq!(options.phlo_price.to_string(), "18446744073709551617");
    assert_eq!(options.phlo_limit, 30_000);
    assert_eq!(resolve(&reported, Some(12_000)).unwrap(), 12_000);
    assert!(resolve(&reported, Some(30_001))
        .unwrap_err()
        .to_string()
        .contains("30001 is above the shard's cap of 30000"));
    assert_eq!(
        resolve(&parameters(None, Some(20_000_000)), None).unwrap(),
        50_000
    );
    assert_eq!(resolve(&parameters(None, None), None).unwrap(), 50_000);
    assert_eq!(
        args.deploy
            .options(50_000, &parameters(Some("0"), Some(30_000)))
            .unwrap()
            .phlo_price,
        BigUint::from(1u8)
    );
    assert!(args
        .deploy
        .options(50_000, &parameters(None, Some(30_000)))
        .is_err());
}

#[cfg(feature = "cli")]
#[test]
fn deploy_reaches_the_node_over_grpc_alone() {
    use clap::Parser;
    use node_cli::args::DeployArgs;

    DeployArgs::try_parse_from([
        "deploy",
        "-f",
        "contract.rho",
        "--grpc",
        "https://validator.example:40401",
    ])
    .expect("deploy takes its node over gRPC alone");
    assert!(
        DeployArgs::try_parse_from([
            "deploy",
            "-f",
            "contract.rho",
            "--http",
            "http://localhost:40403",
        ])
        .is_err(),
        "deploy has no HTTP endpoint to require"
    );
}

#[cfg(feature = "cli")]
#[test]
fn the_automatic_price_never_exceeds_the_caller_ceiling() {
    use clap::Parser;
    use node_cli::args::DeployArgs;

    let args =
        DeployArgs::try_parse_from(["deploy", "-f", "contract.rho", "--max-phlo-price", "10"])
            .unwrap();
    let price = |price: &str| {
        args.deploy
            .options(50_000, &parameters(Some(price), None))
            .unwrap()
            .phlo_price
    };
    assert_eq!(price("9"), BigUint::from(9u8));
    assert_eq!(price("10"), BigUint::from(10u8));
    let error = args
        .deploy
        .options(50_000, &parameters(Some("11"), None))
        .unwrap_err()
        .to_string();
    assert!(error.contains("minimum of 11"), "{error}");
    assert!(error.contains("ceiling of 10"), "{error}");

    assert!(DeployArgs::try_parse_from([
        "deploy",
        "-f",
        "contract.rho",
        "--phlo-price",
        "5",
        "--max-phlo-price",
        "10",
    ])
    .is_err());
}

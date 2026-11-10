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
fn status_and_deploy_prices_are_canonical_uint256_strings() {
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
    assert_eq!(
        node_cli::vault::tokens_to_dust("0.000000000000000001", 18).unwrap(),
        BigUint::from(1u8)
    );
}

#[test]
fn node_encoder_and_verifier_agree_with_client_signatures() {
    use f1r3fly_crypto::rust::signatures::signed::Signed;
    let key = SigningKey::from_hex(&"42".repeat(32)).unwrap();
    for price in [
        BigUint::from(1u8),
        BigUint::from(1u8) << 128usize,
        (BigUint::from(1u8) << 256usize) - 1u8,
    ] {
        let options = DeployOptions {
            timestamp: Some(1000),
            phlo_limit: 1,
            phlo_price: price.clone(),
            ..Default::default()
        };
        let client = UnsignedDeploy::new("Nil", &options, 7, "root")
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
    #[derive(Parser)]
    struct Flags {
        #[command(flatten)]
        options: DeployOptionsArgs,
    }
    assert_eq!(
        Flags::try_parse_from(["price"]).unwrap().options.phlo_price,
        BigUint::from(1u8)
    );
    let wide = (BigUint::from(1u8) << 128usize).to_string();
    assert_eq!(
        Flags::try_parse_from(["price", "--phlo-price", &wide])
            .unwrap()
            .options
            .phlo_price
            .to_string(),
        wide
    );
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

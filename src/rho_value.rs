//! Values a node returns from Rholang: query results and data sent to a name.
//!
//! A [`RhoValue`] is decoded from the protocol's [`Par`] (gRPC) or from the
//! node's HTTP JSON. The HTTP encoding loses information: map keys arrive as
//! strings, and `Nil` elements of collections are dropped.

use std::fmt;

use f1r3fly_models::rhoapi::expr::ExprInstance;
use f1r3fly_models::rhoapi::g_unforgeable::UnfInstance;
use f1r3fly_models::rhoapi::{Bundle, Expr, GUnforgeable, Par, Var};
use num_bigint::{BigInt, BigUint, Sign};
use serde_json::Value;

use crate::error::{Error, Result};

/// A Rholang value.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum RhoValue {
    Nil,
    Bool(bool),
    Int(i64),
    Int32(i32),
    Uint64(u64),
    Uint32(u32),
    Uint16(u16),
    Uint8(u8),
    BigInt(BigInt),
    Float(f64),
    Float32(f32),
    /// `numerator / denominator`, with a positive denominator.
    BigRat {
        numerator: BigInt,
        denominator: BigInt,
    },
    /// `unscaled / 10^scale`.
    FixedPoint {
        unscaled: BigInt,
        scale: u32,
    },
    String(String),
    Uri(String),
    Bytes(Vec<u8>),
    List(Vec<RhoValue>),
    Tuple(Vec<RhoValue>),
    Set(Vec<RhoValue>),
    Map(Vec<(RhoValue, RhoValue)>),
    PathMap(Vec<RhoValue>),
    /// Values in parallel: `a | b`.
    Par(Vec<RhoValue>),
    Unforgeable(Unforgeable),
    Bundle {
        body: Box<RhoValue>,
        read: bool,
        write: bool,
    },
}

/// A name no Rholang term can write down; only evaluation creates one.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Unforgeable {
    Private(Vec<u8>),
    DeployId(Vec<u8>),
    DeployerId(Vec<u8>),
    SysAuthToken,
}

fn not_a_value(what: impl fmt::Display) -> Error {
    Error::decode("value", format!("{what} is not a value"))
}

fn malformed(json: &Value) -> Error {
    Error::decode("value", format!("malformed expression {json}"))
}

impl RhoValue {
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            RhoValue::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            RhoValue::String(s) => Some(s),
            _ => None,
        }
    }

    /// The value of any integer kind.
    pub fn as_integer(&self) -> Option<BigInt> {
        Some(match self {
            RhoValue::Int(n) => BigInt::from(*n),
            RhoValue::Int32(n) => BigInt::from(*n),
            RhoValue::Uint64(n) => BigInt::from(*n),
            RhoValue::Uint32(n) => BigInt::from(*n),
            RhoValue::Uint16(n) => BigInt::from(*n),
            RhoValue::Uint8(n) => BigInt::from(*n),
            RhoValue::BigInt(n) => n.clone(),
            _ => return None,
        })
    }

    /// An integer of any kind that fits in an `i64`.
    pub fn as_i64(&self) -> Option<i64> {
        i64::try_from(self.as_integer()?).ok()
    }

    /// An unsigned uint256 amount, of any integer kind.
    pub fn as_amount(&self) -> Result<BigUint> {
        let amount = self
            .as_integer()
            .and_then(|n| n.to_biguint())
            .ok_or_else(|| Error::decode("amount", format!("{self} is not an unsigned integer")))?;
        if amount.bits() > 256 {
            return Err(Error::decode("amount", format!("{amount} exceeds uint256")));
        }
        Ok(amount)
    }

    /// The elements of a list, tuple or set.
    pub fn elements(&self) -> Option<&[RhoValue]> {
        match self {
            RhoValue::List(items) | RhoValue::Tuple(items) | RhoValue::Set(items) => Some(items),
            _ => None,
        }
    }

    pub fn entries(&self) -> Option<&[(RhoValue, RhoValue)]> {
        match self {
            RhoValue::Map(entries) => Some(entries),
            _ => None,
        }
    }

    /// The value a map holds under the string `key`.
    pub fn get(&self, key: &str) -> Option<&RhoValue> {
        self.entries()?
            .iter()
            .find(|(k, _)| k.as_str() == Some(key))
            .map(|(_, v)| v)
    }

    /// Decode one expression of the node's HTTP JSON, as `/api/explore-deploy`
    /// returns them.
    pub fn from_node_json(json: &Value) -> Result<Self> {
        if let Some(tag) = json.as_str() {
            return match tag {
                "ExprSysAuthToken" => Ok(RhoValue::Unforgeable(Unforgeable::SysAuthToken)),
                _ => Err(malformed(json)),
            };
        }
        let (tag, body) = json
            .as_object()
            .filter(|object| object.len() == 1)
            .and_then(|object| object.iter().next())
            .ok_or_else(|| malformed(json))?;
        let field = |name: &str| body.get(name).ok_or_else(|| malformed(json));
        let data = || field("data");
        let int = |range: &'static str| -> Result<i128> {
            data()?
                .as_i64()
                .map(i128::from)
                .or_else(|| data().ok()?.as_u64().map(i128::from))
                .ok_or_else(|| Error::decode("value", format!("{range} expected, got {json}")))
        };
        let narrow = |range: &'static str| -> Error {
            Error::decode("value", format!("{json} is out of range for {range}"))
        };
        let text = |name: &str| -> Result<String> {
            field(name)?
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| malformed(json))
        };

        Ok(match tag.as_str() {
            "ExprPar" => RhoValue::Par(node_json_list(data()?)?),
            "ExprTuple" => RhoValue::Tuple(node_json_list(data()?)?),
            "ExprList" => RhoValue::List(node_json_list(data()?)?),
            "ExprSet" => RhoValue::Set(node_json_list(data()?)?),
            "ExprMap" => RhoValue::Map(
                data()?
                    .as_object()
                    .ok_or_else(|| malformed(json))?
                    .iter()
                    .map(|(key, value)| {
                        Ok((RhoValue::String(key.clone()), Self::from_node_json(value)?))
                    })
                    .collect::<Result<_>>()?,
            ),
            "ExprBool" => RhoValue::Bool(data()?.as_bool().ok_or_else(|| malformed(json))?),
            "ExprInt" => RhoValue::Int(i64::try_from(int("i64")?).map_err(|_| narrow("i64"))?),
            "ExprInt32" => RhoValue::Int32(i32::try_from(int("i32")?).map_err(|_| narrow("i32"))?),
            "ExprUint64" => {
                RhoValue::Uint64(u64::try_from(int("u64")?).map_err(|_| narrow("u64"))?)
            }
            "ExprUint32" => {
                RhoValue::Uint32(u32::try_from(int("u32")?).map_err(|_| narrow("u32"))?)
            }
            "ExprUint16" => {
                RhoValue::Uint16(u16::try_from(int("u16")?).map_err(|_| narrow("u16"))?)
            }
            "ExprUint8" => RhoValue::Uint8(u8::try_from(int("u8")?).map_err(|_| narrow("u8"))?),
            "ExprFloat" => RhoValue::Float(data()?.as_f64().ok_or_else(|| malformed(json))?),
            "ExprFloat32" => {
                RhoValue::Float32(data()?.as_f64().ok_or_else(|| malformed(json))? as f32)
            }
            "ExprBigInt" => RhoValue::BigInt(canonical_bigint(&text("data")?)?),
            "ExprBigRat" => RhoValue::BigRat {
                numerator: canonical_bigint(&text("numerator")?)?,
                denominator: canonical_bigint(&text("denominator")?)?,
            },
            "ExprFixedPoint" => RhoValue::FixedPoint {
                unscaled: canonical_bigint(&text("value")?)?,
                scale: field("scale")?
                    .as_u64()
                    .and_then(|scale| u32::try_from(scale).ok())
                    .ok_or_else(|| malformed(json))?,
            },
            "ExprString" => RhoValue::String(text("data")?),
            "ExprUri" => RhoValue::Uri(text("data")?),
            "ExprBytes" => RhoValue::Bytes(hex_field(&text("data")?, json)?),
            "ExprUnforg" => RhoValue::Unforgeable(node_json_unforgeable(data()?)?),
            "ExprBundle" => RhoValue::Bundle {
                body: Box::new(Self::from_node_json(data()?)?),
                read: field("read")?.as_bool().ok_or_else(|| malformed(json))?,
                write: field("write")?.as_bool().ok_or_else(|| malformed(json))?,
            },
            // The node writes an absent operand or bundle body, which is the
            // empty process, as an unknown "Nil".
            "ExprUnknown" => match field("type_name")?.as_str() {
                Some("Nil") => RhoValue::Nil,
                Some("Process") => return Err(not_a_value("a process")),
                Some(kind) => return Err(not_a_value(format!("a {kind}"))),
                None => return Err(malformed(json)),
            },
            other => return Err(not_a_value(format!("an unevaluated expression ({other})"))),
        })
    }
}

fn node_json_list(data: &Value) -> Result<Vec<RhoValue>> {
    data.as_array()
        .ok_or_else(|| malformed(data))?
        .iter()
        .map(RhoValue::from_node_json)
        .collect()
}

fn node_json_unforgeable(data: &Value) -> Result<Unforgeable> {
    if data.as_str() == Some("UnforgSysAuthToken") {
        return Ok(Unforgeable::SysAuthToken);
    }
    let (kind, body) = data
        .as_object()
        .filter(|object| object.len() == 1)
        .and_then(|object| object.iter().next())
        .ok_or_else(|| malformed(data))?;
    let id = hex_field(
        body.get("data")
            .and_then(Value::as_str)
            .ok_or_else(|| malformed(data))?,
        data,
    )?;
    Ok(match kind.as_str() {
        "UnforgPrivate" => Unforgeable::Private(id),
        "UnforgDeploy" => Unforgeable::DeployId(id),
        "UnforgDeployer" => Unforgeable::DeployerId(id),
        _ => return Err(malformed(data)),
    })
}

fn hex_field(text: &str, json: &Value) -> Result<Vec<u8>> {
    hex::decode(text).map_err(|e| Error::decode("value", format!("{json}: {e}")))
}

/// The node writes a BigInt as its canonical decimal string; anything else is malformed.
fn canonical_bigint(text: &str) -> Result<BigInt> {
    let value = text
        .parse::<BigInt>()
        .map_err(|e| Error::decode("BigInt", format!("{text}: {e}")))?;
    if value.to_string() != text {
        return Err(Error::decode("BigInt", format!("{text} is not canonical")));
    }
    Ok(value)
}

impl TryFrom<&Par> for RhoValue {
    type Error = Error;

    fn try_from(par: &Par) -> Result<Self> {
        if !par.sends.is_empty()
            || !par.receives.is_empty()
            || !par.news.is_empty()
            || !par.matches.is_empty()
            || !par.conditionals.is_empty()
            || !par.connectives.is_empty()
        {
            return Err(not_a_value("a process"));
        }
        let mut values = par
            .exprs
            .iter()
            .map(from_expr)
            .chain(par.unforgeables.iter().map(from_unforgeable))
            .chain(par.bundles.iter().map(from_bundle))
            .collect::<Result<Vec<_>>>()?;
        Ok(match values.len() {
            0 => RhoValue::Nil,
            1 => values.pop().expect("one value"),
            _ => RhoValue::Par(values),
        })
    }
}

/// An absent `Par` field is the empty process, `Nil`.
fn optional(par: &Option<Par>) -> Result<RhoValue> {
    par.as_ref().map_or(Ok(RhoValue::Nil), RhoValue::try_from)
}

fn all(pars: &[Par]) -> Result<Vec<RhoValue>> {
    pars.iter().map(RhoValue::try_from).collect()
}

/// A collection with a remainder is a pattern, such as `[head ...tail]`.
fn complete(remainder: &Option<Var>) -> Result<()> {
    match remainder {
        Some(_) => Err(not_a_value("a pattern")),
        None => Ok(()),
    }
}

fn from_expr(expr: &Expr) -> Result<RhoValue> {
    let instance = expr
        .expr_instance
        .as_ref()
        .ok_or_else(|| not_a_value("an expression of a kind this client does not know"))?;
    let narrow =
        |n: u32, range: &str| Error::decode("value", format!("{n} is out of range for {range}"));
    Ok(match instance {
        ExprInstance::GBool(b) => RhoValue::Bool(*b),
        ExprInstance::GInt(n) => RhoValue::Int(*n),
        ExprInstance::GInt32(n) => RhoValue::Int32(*n),
        ExprInstance::GUint64(n) => RhoValue::Uint64(*n),
        ExprInstance::GUint32(n) => RhoValue::Uint32(*n),
        ExprInstance::GUint16(n) => {
            RhoValue::Uint16(u16::try_from(*n).map_err(|_| narrow(*n, "u16"))?)
        }
        ExprInstance::GUint8(n) => RhoValue::Uint8(u8::try_from(*n).map_err(|_| narrow(*n, "u8"))?),
        ExprInstance::GBigInt(bytes) => RhoValue::BigInt(BigInt::from_signed_bytes_be(bytes)),
        ExprInstance::GDouble(bits) => RhoValue::Float(f64::from_bits(*bits)),
        ExprInstance::GFloat32(bits) => RhoValue::Float32(f32::from_bits(*bits)),
        ExprInstance::GBigRat(rat) => RhoValue::BigRat {
            numerator: BigInt::from_signed_bytes_be(&rat.numerator),
            denominator: BigInt::from_signed_bytes_be(&rat.denominator),
        },
        ExprInstance::GFixedPoint(fixed) => RhoValue::FixedPoint {
            unscaled: BigInt::from_signed_bytes_be(&fixed.unscaled),
            scale: fixed.scale,
        },
        ExprInstance::GString(s) => RhoValue::String(s.clone()),
        ExprInstance::GUri(uri) => RhoValue::Uri(uri.clone()),
        ExprInstance::GByteArray(bytes) => RhoValue::Bytes(bytes.clone()),
        ExprInstance::EListBody(list) => {
            complete(&list.remainder)?;
            RhoValue::List(all(&list.ps)?)
        }
        ExprInstance::ETupleBody(tuple) => RhoValue::Tuple(all(&tuple.ps)?),
        ExprInstance::ESetBody(set) => {
            complete(&set.remainder)?;
            RhoValue::Set(all(&set.ps)?)
        }
        ExprInstance::EMapBody(map) => {
            complete(&map.remainder)?;
            RhoValue::Map(
                map.kvs
                    .iter()
                    .map(|kv| Ok((optional(&kv.key)?, optional(&kv.value)?)))
                    .collect::<Result<_>>()?,
            )
        }
        ExprInstance::EPathmapBody(pathmap) => {
            complete(&pathmap.remainder)?;
            RhoValue::PathMap(all(&pathmap.ps)?)
        }
        ExprInstance::EZipperBody(_) => return Err(not_a_value("a PathMap zipper")),
        ExprInstance::EVarBody(_) => return Err(not_a_value("a variable")),
        _ => return Err(not_a_value("an unevaluated expression")),
    })
}

fn from_unforgeable(name: &GUnforgeable) -> Result<RhoValue> {
    let instance = name
        .unf_instance
        .as_ref()
        .ok_or_else(|| not_a_value("an unforgeable name of a kind this client does not know"))?;
    Ok(RhoValue::Unforgeable(match instance {
        UnfInstance::GPrivateBody(private) => Unforgeable::Private(private.id.clone()),
        UnfInstance::GDeployIdBody(deploy) => Unforgeable::DeployId(deploy.sig.clone()),
        UnfInstance::GDeployerIdBody(deployer) => {
            Unforgeable::DeployerId(deployer.public_key.clone())
        }
        UnfInstance::GSysAuthTokenBody(_) => Unforgeable::SysAuthToken,
    }))
}

fn from_bundle(bundle: &Bundle) -> Result<RhoValue> {
    Ok(RhoValue::Bundle {
        body: Box::new(optional(&bundle.body)?),
        read: bundle.read_flag,
        write: bundle.write_flag,
    })
}

/// Rholang source syntax, where the value has one.
impl fmt::Display for RhoValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RhoValue::Nil => f.write_str("Nil"),
            RhoValue::Bool(b) => write!(f, "{b}"),
            RhoValue::Int(n) => write!(f, "{n}"),
            RhoValue::Int32(n) => write!(f, "{n}i32"),
            RhoValue::Uint64(n) => write!(f, "{n}u64"),
            RhoValue::Uint32(n) => write!(f, "{n}u32"),
            RhoValue::Uint16(n) => write!(f, "{n}u16"),
            RhoValue::Uint8(n) => write!(f, "{n}u8"),
            RhoValue::BigInt(n) => write!(f, "{n}n"),
            RhoValue::Float(x) if x.is_finite() && *x == x.floor() => write!(f, "{x:.1}f64"),
            RhoValue::Float(x) => write!(f, "{x}f64"),
            RhoValue::Float32(x) if x.is_finite() && *x == x.floor() => write!(f, "{x:.1}f32"),
            RhoValue::Float32(x) => write!(f, "{x}f32"),
            RhoValue::BigRat {
                numerator,
                denominator,
            } => write!(f, "{numerator}/{denominator}r"),
            RhoValue::FixedPoint { unscaled, scale } => {
                let sign = if unscaled.sign() == Sign::Minus {
                    "-"
                } else {
                    ""
                };
                let digits = unscaled.magnitude().to_string();
                let scale_digits = *scale as usize;
                if scale_digits == 0 {
                    return write!(f, "{sign}{digits}p0");
                }
                let padded = format!("{digits:0>width$}", width = scale_digits + 1);
                let (whole, fraction) = padded.split_at(padded.len() - scale_digits);
                write!(f, "{sign}{whole}.{fraction}p{scale}")
            }
            RhoValue::String(s) => write!(f, "{s:?}"),
            RhoValue::Uri(uri) => write!(f, "`{uri}`"),
            RhoValue::Bytes(bytes) => write!(f, "0x{}", hex::encode(bytes)),
            RhoValue::List(items) => write!(f, "[{}]", Joined(items, ", ")),
            RhoValue::Tuple(items) if items.len() == 1 => write!(f, "({},)", items[0]),
            RhoValue::Tuple(items) => write!(f, "({})", Joined(items, ", ")),
            RhoValue::Set(items) => write!(f, "Set({})", Joined(items, ", ")),
            RhoValue::Map(entries) => {
                f.write_str("{")?;
                for (i, (key, value)) in entries.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{key}: {value}")?;
                }
                f.write_str("}")
            }
            RhoValue::PathMap(items) => write!(f, "{{| {} |}}", Joined(items, ", ")),
            RhoValue::Par(items) => write!(f, "{}", Joined(items, " | ")),
            RhoValue::Unforgeable(name) => match name {
                Unforgeable::Private(id) => write!(f, "Unforgeable(0x{})", hex::encode(id)),
                Unforgeable::DeployId(id) => write!(f, "DeployId(0x{})", hex::encode(id)),
                Unforgeable::DeployerId(id) => write!(f, "DeployerId(0x{})", hex::encode(id)),
                Unforgeable::SysAuthToken => f.write_str("SysAuthToken"),
            },
            RhoValue::Bundle { body, read, write } => {
                let kind = match (read, write) {
                    (true, true) => "bundle",
                    (false, true) => "bundle+",
                    (true, false) => "bundle-",
                    (false, false) => "bundle0",
                };
                write!(f, "{kind} {{ {body} }}")
            }
        }
    }
}

struct Joined<'a>(&'a [RhoValue], &'static str);

impl fmt::Display for Joined<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, item) in self.0.iter().enumerate() {
            if i > 0 {
                f.write_str(self.1)?;
            }
            write!(f, "{item}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use f1r3fly_models::rhoapi::{
        EList, EMap, EPlus, ETuple, GBigRational, GFixedPoint, GPrivate, KeyValuePair, Send,
    };
    use serde_json::json;

    fn par(instance: ExprInstance) -> Par {
        Par {
            exprs: vec![Expr {
                expr_instance: Some(instance),
            }],
            ..Default::default()
        }
    }

    fn decode(instance: ExprInstance) -> Result<RhoValue> {
        RhoValue::try_from(&par(instance))
    }

    #[test]
    fn every_value_kind_prints_as_rholang_source() {
        let big: BigInt = (BigInt::from(1u8) << 200usize) + 1u8;
        for (instance, text) in [
            (ExprInstance::GInt(-5), "-5".to_string()),
            (ExprInstance::GInt32(-5), "-5i32".into()),
            (
                ExprInstance::GUint64(u64::MAX),
                "18446744073709551615u64".into(),
            ),
            (ExprInstance::GUint16(65535), "65535u16".into()),
            (ExprInstance::GUint8(255), "255u8".into()),
            (
                ExprInstance::GBigInt(big.to_signed_bytes_be()),
                format!("{big}n"),
            ),
            (ExprInstance::GBigInt(vec![]), "0n".into()),
            (ExprInstance::GDouble(2.0f64.to_bits()), "2.0f64".into()),
            (ExprInstance::GFloat32(0.1f32.to_bits()), "0.1f32".into()),
            (
                ExprInstance::GBigRat(GBigRational {
                    numerator: vec![1],
                    denominator: vec![2],
                }),
                "1/2r".into(),
            ),
            (
                ExprInstance::GFixedPoint(GFixedPoint {
                    unscaled: vec![5],
                    scale: 3,
                }),
                "0.005p3".into(),
            ),
            (
                ExprInstance::GFixedPoint(GFixedPoint {
                    unscaled: BigInt::from(-12345).to_signed_bytes_be(),
                    scale: 2,
                }),
                "-123.45p2".into(),
            ),
            (ExprInstance::GString("a\"b".into()), r#""a\"b""#.into()),
            (ExprInstance::GUri("rho:id:x".into()), "`rho:id:x`".into()),
            (ExprInstance::GByteArray(vec![0xde, 0xad]), "0xdead".into()),
            (
                ExprInstance::ETupleBody(ETuple {
                    ps: vec![par(ExprInstance::GBool(true))],
                    ..Default::default()
                }),
                "(true,)".into(),
            ),
        ] {
            assert_eq!(decode(instance).unwrap().to_string(), text);
        }
    }

    #[test]
    fn nested_values_keep_nil_elements_and_non_string_keys() {
        let map = decode(ExprInstance::EMapBody(EMap {
            kvs: vec![KeyValuePair {
                key: Some(par(ExprInstance::GInt(1))),
                value: Some(par(ExprInstance::EListBody(EList {
                    ps: vec![Par::default(), par(ExprInstance::GString("x".into()))],
                    ..Default::default()
                }))),
            }],
            ..Default::default()
        }))
        .unwrap();
        assert_eq!(
            map,
            RhoValue::Map(vec![(
                RhoValue::Int(1),
                RhoValue::List(vec![RhoValue::Nil, RhoValue::String("x".into())])
            )])
        );
        assert_eq!(map.to_string(), r#"{1: [Nil, "x"]}"#);
    }

    #[test]
    fn names_and_parallel_values_are_values() {
        let value = RhoValue::try_from(&Par {
            exprs: vec![Expr {
                expr_instance: Some(ExprInstance::GInt(1)),
            }],
            unforgeables: vec![GUnforgeable {
                unf_instance: Some(UnfInstance::GPrivateBody(GPrivate { id: vec![0xab] })),
            }],
            bundles: vec![Bundle {
                body: None,
                write_flag: true,
                read_flag: false,
            }],
            ..Default::default()
        })
        .unwrap();
        assert_eq!(value.to_string(), "1 | Unforgeable(0xab) | bundle+ { Nil }");
    }

    #[test]
    fn processes_patterns_and_expressions_are_not_values() {
        let process = Par {
            sends: vec![Send::default()],
            ..Default::default()
        };
        assert!(RhoValue::try_from(&process).is_err());
        assert!(decode(ExprInstance::EPlusBody(EPlus::default())).is_err());
        assert!(decode(ExprInstance::EListBody(EList {
            remainder: Some(Var::default()),
            ..Default::default()
        }))
        .is_err());
        assert!(decode(ExprInstance::GUint16(65536)).is_err());
        assert!(RhoValue::try_from(&Par {
            exprs: vec![Expr::default()],
            ..Default::default()
        })
        .is_err());
    }

    #[test]
    fn node_json_decodes_every_value_kind() {
        let value = RhoValue::from_node_json(&json!({"ExprTuple": {"data": [
            {"ExprBigInt": {"data": "-18446744073709551617"}},
            {"ExprUint8": {"data": 255}},
            {"ExprFloat": {"data": 2.5}},
            {"ExprFixedPoint": {"value": "12345", "scale": 2}},
            {"ExprBigRat": {"numerator": "1", "denominator": "3"}},
            {"ExprUri": {"data": "rho:id:x"}},
            {"ExprBytes": {"data": "dead"}},
            {"ExprSet": {"data": [{"ExprBool": {"data": true}}]}},
            {"ExprMap": {"data": {"k": {"ExprString": {"data": "v"}}}}},
            {"ExprUnforg": {"data": {"UnforgDeployer": {"data": "04ab"}}}},
            {"ExprUnforg": {"data": "UnforgSysAuthToken"}},
            {"ExprBundle": {"data": {"ExprUnknown": {"type_name": "Nil"}}, "read": true, "write": false}},
            {"ExprPar": {"data": [{"ExprInt": {"data": 1}}, {"ExprInt32": {"data": -2}}]}}
        ]}}))
        .unwrap();
        assert_eq!(
            value.to_string(),
            "(-18446744073709551617n, 255u8, 2.5f64, 123.45p2, 1/3r, `rho:id:x`, 0xdead, \
             Set(true), {\"k\": \"v\"}, DeployerId(0x04ab), SysAuthToken, bundle- { Nil }, \
             1 | -2i32)"
        );
        assert_eq!(
            value.elements().unwrap()[8].get("k").unwrap().as_str(),
            Some("v")
        );
    }

    #[test]
    fn node_json_refuses_what_is_not_a_canonical_value() {
        for json in [
            json!({"ExprBigInt": {"data": "01"}}),
            json!({"ExprBigInt": {"data": 42}}),
            json!({"ExprUint8": {"data": 256}}),
            json!({"ExprInt32": {"data": 2147483648i64}}),
            json!({"ExprFloat": {"data": null}}),
            json!({"ExprBytes": {"data": "xyz"}}),
            json!({"ExprPlus": {"left": {}, "right": {}}}),
            json!({"ExprUnknown": {"type_name": "Process"}}),
            json!({"ExprInt": {"data": 1}, "ExprBool": {"data": true}}),
            json!("ExprSomethingNew"),
        ] {
            assert!(RhoValue::from_node_json(&json).is_err(), "{json}");
        }
    }

    #[test]
    fn amounts_are_unsigned_uint256_integers_of_any_kind() {
        assert_eq!(RhoValue::Uint64(7).as_amount().unwrap(), BigUint::from(7u8));
        let max: BigInt = (BigInt::from(1u8) << 256usize) - 1u8;
        assert!(RhoValue::BigInt(max.clone()).as_amount().is_ok());
        for value in [
            RhoValue::BigInt(max + 1u8),
            RhoValue::Int(-1),
            RhoValue::String("1000".into()),
        ] {
            assert!(value.as_amount().is_err(), "{value}");
        }
    }
}

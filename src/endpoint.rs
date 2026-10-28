//! Node endpoints, and the TLS settings each transport uses to reach them.

use std::fmt;
use std::str::FromStr;
use std::sync::Arc;

use reqwest::Url;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::CertificateDer;

use crate::error::{Error, Result};

/// Where one node API answers: `http://host:port` for plaintext, or
/// `https://host:port` for a node behind TLS.
///
/// An `https` endpoint trusts the bundled public root certificates unless it
/// carries its own certificate authorities, in which case it trusts only those.
#[derive(Clone, PartialEq, Eq)]
pub struct Endpoint {
    url: Url,
    ca_pem: Option<Arc<[u8]>>,
}

impl Endpoint {
    /// An endpoint from its URL. The scheme must be `http` or `https`, and the
    /// URL names only a host and port: no path, query or credentials.
    pub fn parse(url: &str) -> Result<Self> {
        let invalid = |detail: &str| Error::invalid("endpoint", format!("{url}: {detail}"));
        let parsed = Url::parse(url).map_err(|e| invalid(&e.to_string()))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(invalid("the scheme must be http or https"));
        }
        if parsed.host_str().is_none() {
            return Err(invalid("no host"));
        }
        if parsed.path() != "/" || parsed.query().is_some() || parsed.fragment().is_some() {
            return Err(invalid("an endpoint has no path, query or fragment"));
        }
        if !parsed.username().is_empty() || parsed.password().is_some() {
            return Err(invalid("an endpoint carries no credentials"));
        }
        Ok(Endpoint {
            url: parsed,
            ca_pem: None,
        })
    }

    /// Trust only the certificate authorities in `pem`, one or more PEM
    /// certificates, instead of the bundled public roots. Applies to `https`
    /// endpoints; a plaintext endpoint ignores it.
    pub fn with_ca_certificates(mut self, pem: impl Into<Vec<u8>>) -> Result<Self> {
        let pem: Vec<u8> = pem.into();
        parse_certificates(&pem)?;
        self.ca_pem = Some(pem.into());
        Ok(self)
    }

    pub fn url(&self) -> &Url {
        &self.url
    }

    pub fn is_tls(&self) -> bool {
        self.url.scheme() == "https"
    }

    /// `path`, which starts with `/`, on this endpoint.
    pub(crate) fn join(&self, path: &str) -> String {
        format!("{self}{path}")
    }

    /// `path` on this endpoint over WebSocket: `wss` when the endpoint is
    /// `https`, `ws` otherwise.
    pub(crate) fn websocket_url(&self, path: &str) -> String {
        let scheme = if self.is_tls() { "wss" } else { "ws" };
        format!("{scheme}://{}{path}", self.authority())
    }

    fn authority(&self) -> String {
        let host = self.url.host_str().expect("checked by parse");
        match self.url.port() {
            Some(port) => format!("{host}:{port}"),
            None => host.to_string(),
        }
    }

    /// TLS for a gRPC channel, or `None` for plaintext.
    pub(crate) fn grpc_tls(&self) -> Option<tonic::transport::ClientTlsConfig> {
        if !self.is_tls() {
            return None;
        }
        let config = tonic::transport::ClientTlsConfig::new();
        Some(match &self.ca_pem {
            Some(pem) => config.ca_certificate(tonic::transport::Certificate::from_pem(pem)),
            None => config.with_webpki_roots(),
        })
    }

    /// An HTTP client that trusts what this endpoint trusts.
    pub(crate) fn http_client(&self) -> Result<reqwest::Client> {
        let mut builder = reqwest::Client::builder();
        if let Some(pem) = &self.ca_pem {
            builder = builder.tls_built_in_root_certs(false);
            for certificate in reqwest::Certificate::from_pem_bundle(pem)
                .map_err(|e| Error::invalid("CA certificate", e))?
            {
                builder = builder.add_root_certificate(certificate);
            }
        }
        builder
            .build()
            .map_err(|e| Error::invalid("HTTP client", e))
    }

    /// TLS for a WebSocket connection to this endpoint.
    pub(crate) fn websocket_tls(&self) -> Result<rustls::ClientConfig> {
        let mut roots = rustls::RootCertStore::empty();
        match &self.ca_pem {
            Some(pem) => {
                for certificate in parse_certificates(pem)? {
                    roots
                        .add(certificate)
                        .map_err(|e| Error::invalid("CA certificate", e))?;
                }
            }
            None => roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned()),
        }
        Ok(rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .map_err(|e| Error::invalid("TLS configuration", e))?
        .with_root_certificates(roots)
        .with_no_client_auth())
    }
}

/// An open WebSocket to a node.
pub(crate) type WebSocket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// Open a WebSocket to `path` on `endpoint`, over TLS when it is `https`.
pub(crate) async fn connect_websocket(endpoint: &Endpoint, path: &str) -> Result<WebSocket> {
    let url = endpoint.websocket_url(path);
    let connector = if endpoint.is_tls() {
        tokio_tungstenite::Connector::Rustls(Arc::new(endpoint.websocket_tls()?))
    } else {
        tokio_tungstenite::Connector::Plain
    };
    let (stream, _) =
        tokio_tungstenite::connect_async_tls_with_config(&url, None, false, Some(connector))
            .await
            .map_err(|e| Error::transport(&url, e))?;
    Ok(stream)
}

fn parse_certificates(pem: &[u8]) -> Result<Vec<CertificateDer<'static>>> {
    let certificates = CertificateDer::pem_slice_iter(pem)
        .collect::<std::result::Result<Vec<_>, _>>()
        .map_err(|e| Error::invalid("CA certificate", e))?;
    if certificates.is_empty() {
        return Err(Error::invalid(
            "CA certificate",
            "no PEM certificate in the bundle",
        ));
    }
    Ok(certificates)
}

impl FromStr for Endpoint {
    type Err = Error;

    fn from_str(url: &str) -> Result<Self> {
        Endpoint::parse(url)
    }
}

/// The endpoint's origin, such as `https://node.example:40401`.
impl fmt::Display for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}://{}", self.url.scheme(), self.authority())
    }
}

impl fmt::Debug for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Endpoint")
            .field("url", &self.to_string())
            .field("custom_ca", &self.ca_pem.is_some())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_endpoint_keeps_its_scheme_host_and_port() {
        let plain = Endpoint::parse("http://localhost:40401").unwrap();
        assert!(!plain.is_tls());
        assert_eq!(plain.to_string(), "http://localhost:40401");
        assert_eq!(
            plain.join("/api/status"),
            "http://localhost:40401/api/status"
        );
        assert_eq!(
            plain.websocket_url("/ws/events"),
            "ws://localhost:40401/ws/events"
        );

        let secure = Endpoint::parse("https://node.example").unwrap();
        assert!(secure.is_tls());
        assert_eq!(secure.to_string(), "https://node.example");
        assert_eq!(
            secure.websocket_url("/ws/events"),
            "wss://node.example/ws/events"
        );
    }

    #[test]
    fn only_tls_endpoints_get_grpc_tls() {
        assert!(Endpoint::parse("http://localhost:40401")
            .unwrap()
            .grpc_tls()
            .is_none());
        assert!(Endpoint::parse("https://localhost:40401")
            .unwrap()
            .grpc_tls()
            .is_some());
    }

    #[test]
    fn an_endpoint_is_only_a_scheme_host_and_port() {
        for url in [
            "localhost:40401",
            "ftp://localhost:40401",
            "http://localhost:40401/api",
            "http://localhost:40401?x=1",
            "http://user:pass@localhost:40401",
            "http://",
        ] {
            assert!(
                matches!(
                    Endpoint::parse(url),
                    Err(Error::InvalidInput {
                        what: "endpoint",
                        ..
                    })
                ),
                "{url} was accepted"
            );
        }
    }

    #[test]
    fn a_ca_bundle_must_hold_a_certificate() {
        let endpoint = Endpoint::parse("https://localhost:40401").unwrap();
        assert!(matches!(
            endpoint.clone().with_ca_certificates("not a certificate"),
            Err(Error::InvalidInput {
                what: "CA certificate",
                ..
            })
        ));

        let ca = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let trusted = endpoint.with_ca_certificates(ca.cert.pem()).unwrap();
        assert!(trusted.http_client().is_ok());
        assert!(trusted.websocket_tls().is_ok());
    }
}

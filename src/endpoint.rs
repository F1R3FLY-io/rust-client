//! Node endpoints, and the TLS and timeout settings each transport uses to
//! reach them.

use std::fmt;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;

use reqwest::Url;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::CertificateDer;

use crate::error::{Error, Result, TransportError};

/// How long establishing a connection may take, unless an endpoint sets its own.
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// How long one request may take, unless an endpoint sets its own.
pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Where one node API answers: `http://host:port` for plaintext, or
/// `https://host:port` for a node behind TLS. An HTTP endpoint may carry a
/// base path, such as `https://proxy.example/node1`, for a node served under
/// a prefix; a gRPC endpoint may not.
///
/// An `https` endpoint trusts the bundled public root certificates unless it
/// carries its own certificate authorities, in which case it trusts only those.
///
/// Connecting is bounded by the connect timeout and each request by the
/// request timeout. A WebSocket stream, once open, has no request timeout.
#[derive(Clone, PartialEq, Eq)]
pub struct Endpoint {
    url: Url,
    /// The URL's path without a trailing `/`: empty, or such as `/node1`.
    base_path: String,
    ca_pem: Option<Arc<[u8]>>,
    connect_timeout: Duration,
    request_timeout: Duration,
}

impl Endpoint {
    /// An endpoint from its URL. The scheme must be `http` or `https`; the URL
    /// has a host, an optional port and an optional base path, and no query,
    /// fragment or credentials.
    pub fn parse(url: &str) -> Result<Self> {
        let invalid = |detail: &str| Error::invalid("endpoint", format!("{url}: {detail}"));
        let parsed = Url::parse(url).map_err(|e| invalid(&e.to_string()))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(invalid("the scheme must be http or https"));
        }
        if parsed.host_str().is_none() {
            return Err(invalid("no host"));
        }
        if parsed.query().is_some() || parsed.fragment().is_some() {
            return Err(invalid("an endpoint has no query or fragment"));
        }
        if !parsed.username().is_empty() || parsed.password().is_some() {
            return Err(invalid("an endpoint carries no credentials"));
        }
        let base_path = parsed.path().trim_end_matches('/').to_string();
        Ok(Endpoint {
            url: parsed,
            base_path,
            ca_pem: None,
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
        })
    }

    pub fn with_connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }

    pub fn with_request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = timeout;
        self
    }

    pub fn connect_timeout(&self) -> Duration {
        self.connect_timeout
    }

    pub fn request_timeout(&self) -> Duration {
        self.request_timeout
    }

    /// Whether the endpoint carries a base path, which gRPC cannot use.
    pub fn has_path(&self) -> bool {
        !self.base_path.is_empty()
    }

    /// Trust only the certificate authorities in `pem`, one or more PEM
    /// certificates, instead of the bundled public roots. A plaintext endpoint
    /// is refused: a CA there means TLS was intended but `http` was written.
    pub fn with_ca_certificates(mut self, pem: impl Into<Vec<u8>>) -> Result<Self> {
        if !self.is_tls() {
            return Err(Error::invalid(
                "CA certificate",
                format!("{self} is plaintext; a CA certificate applies only to https endpoints"),
            ));
        }
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
        format!("{scheme}://{}{}{path}", self.authority(), self.base_path)
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

    /// An HTTP client that trusts what this endpoint trusts, within its timeouts.
    pub(crate) fn http_client(&self) -> Result<reqwest::Client> {
        let mut builder = reqwest::Client::builder()
            .connect_timeout(self.connect_timeout)
            .timeout(self.request_timeout);
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

/// Open a WebSocket to `path` on `endpoint`, over TLS when it is `https`,
/// within the endpoint's connect timeout.
pub(crate) async fn connect_websocket(endpoint: &Endpoint, path: &str) -> Result<WebSocket> {
    let url = endpoint.websocket_url(path);
    let connector = if endpoint.is_tls() {
        tokio_tungstenite::Connector::Rustls(Arc::new(endpoint.websocket_tls()?))
    } else {
        tokio_tungstenite::Connector::Plain
    };
    let connect =
        tokio_tungstenite::connect_async_tls_with_config(&url, None, false, Some(connector));
    let (stream, _) = tokio::time::timeout(endpoint.connect_timeout, connect)
        .await
        .map_err(|_| Error::transport(&url, TransportError::TimedOut(endpoint.connect_timeout)))?
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

/// The endpoint's base URL, such as `https://node.example:40401` or
/// `https://proxy.example/node1`.
impl fmt::Display for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}://{}{}",
            self.url.scheme(),
            self.authority(),
            self.base_path
        )
    }
}

impl fmt::Debug for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Endpoint")
            .field("url", &self.to_string())
            .field("custom_ca", &self.ca_pem.is_some())
            .field("connect_timeout", &self.connect_timeout)
            .field("request_timeout", &self.request_timeout)
            .finish()
    }
}

#[cfg(test)]
pub(crate) mod tests {
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
    fn a_base_path_prefixes_every_request() {
        for url in [
            "https://proxy.example/node1",
            "https://proxy.example/node1/",
        ] {
            let endpoint = Endpoint::parse(url).unwrap();
            assert!(endpoint.has_path());
            assert_eq!(endpoint.to_string(), "https://proxy.example/node1");
            assert_eq!(
                endpoint.join("/api/status"),
                "https://proxy.example/node1/api/status"
            );
            assert_eq!(
                endpoint.websocket_url("/ws/events"),
                "wss://proxy.example/node1/ws/events"
            );
        }
        assert!(!Endpoint::parse("http://localhost:40403/")
            .unwrap()
            .has_path());
    }

    #[test]
    fn an_endpoint_has_no_query_fragment_or_credentials() {
        for url in [
            "localhost:40401",
            "ftp://localhost:40401",
            "http://localhost:40401#top",
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

    #[test]
    fn a_ca_on_a_plaintext_endpoint_is_refused() {
        let ca = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let error = Endpoint::parse("http://localhost:40401")
            .unwrap()
            .with_ca_certificates(ca.cert.pem())
            .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("http://localhost:40401 is plaintext"),
            "{error}"
        );
    }

    /// A listener that accepts connections and never answers on them, like a
    /// node that has hung.
    pub(crate) async fn silent_listener() -> Endpoint {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            let mut held = Vec::new();
            while let Ok((socket, _)) = listener.accept().await {
                held.push(socket);
            }
        });
        Endpoint::parse(&format!("http://127.0.0.1:{port}"))
            .unwrap()
            .with_connect_timeout(Duration::from_millis(300))
            .with_request_timeout(Duration::from_millis(300))
    }

    #[tokio::test]
    async fn a_silent_node_fails_an_http_request_within_its_timeout() {
        let endpoint = silent_listener().await;
        let started = std::time::Instant::now();
        let result = endpoint
            .http_client()
            .unwrap()
            .get(endpoint.join("/api/status"))
            .send()
            .await;
        assert!(result.unwrap_err().is_timeout());
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[tokio::test]
    async fn a_silent_node_fails_a_websocket_connect_within_its_timeout() {
        let endpoint = silent_listener().await;
        let started = std::time::Instant::now();
        let result = connect_websocket(&endpoint, "/ws/events").await;
        assert!(matches!(
            result,
            Err(Error::Transport {
                cause: TransportError::TimedOut(_),
                ..
            })
        ));
        assert!(started.elapsed() < Duration::from_secs(3));
    }
}

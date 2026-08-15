//! Per-download HTTP proxy that enforces the SSRF policy on every connection.
//!
//! HTTPS requests arrive as CONNECT tunnels, so TLS remains end-to-end between
//! yt-dlp and the media host. This proxy only resolves the requested host,
//! rejects non-public answers, and connects directly to the validated
//! `SocketAddr`; that pins DNS for the lifetime of each connection.

use std::convert::Infallible;
use std::error::Error;
use std::fmt;
use std::future::Future;
use std::io;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context, Result};
use bytes::Bytes;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::client::conn::http1 as client_http1;
use hyper::header::{HeaderValue, HOST, PROXY_AUTHORIZATION};
use hyper::http::uri::Authority;
use hyper::server::conn::http1 as server_http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode, Uri};
use hyper_util::rt::TokioIo;
use tokio::io::copy_bidirectional;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::task::{JoinHandle, JoinSet};
use tokio::time::{timeout, Instant};
use tokio_util::sync::CancellationToken;

use super::net::{
    is_allowed_host, is_allowed_port, normalize_host, resolved_ips_are_public, DNS_TIMEOUT,
};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const CONNECT_ATTEMPT_TIMEOUT: Duration = Duration::from_secs(2);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_TARGET_ADDRESSES: usize = 16;
const MAX_CONNECTIONS: usize = 64;
const BLOCK_STATUS_CODE: u16 = 472;
pub(super) const BLOCK_MESSAGE: &str = "blocked by video-kadr egress policy";

type BoxError = Box<dyn Error + Send + Sync>;
type ProxyBody = BoxBody<Bytes, BoxError>;
type ConnectFuture =
    Pin<Box<dyn Future<Output = std::result::Result<TcpStream, ConnectError>> + Send>>;
type TargetConnector = Arc<dyn Fn(String, u16) -> ConnectFuture + Send + Sync>;

#[derive(Debug)]
enum ConnectError {
    Blocked,
    Unavailable(String),
}

impl fmt::Display for ConnectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConnectError::Blocked => f.write_str(BLOCK_MESSAGE),
            ConnectError::Unavailable(message) => f.write_str(message),
        }
    }
}

impl Error for ConnectError {}

/// A loopback-only proxy dedicated to one yt-dlp invocation.
pub(super) struct EgressProxy {
    address: SocketAddr,
    blocked: Arc<AtomicBool>,
    cancel: CancellationToken,
    task: Option<JoinHandle<()>>,
}

impl EgressProxy {
    pub(super) async fn start() -> Result<Self> {
        Self::start_with_connector(public_connector()).await
    }

    async fn start_with_connector(connector: TargetConnector) -> Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .await
            .context("bind yt-dlp egress proxy")?;
        let address = listener.local_addr().context("read egress proxy address")?;
        let blocked = Arc::new(AtomicBool::new(false));
        let tracked_blocked = blocked.clone();
        let tracked_connector: TargetConnector = Arc::new(move |host, port| {
            let connect = connector(host, port);
            let blocked = tracked_blocked.clone();
            Box::pin(async move {
                let result = connect.await;
                if matches!(result, Err(ConnectError::Blocked)) {
                    blocked.store(true, Ordering::SeqCst);
                }
                result
            })
        });
        let cancel = CancellationToken::new();
        let task_cancel = cancel.clone();
        let task = tokio::spawn(async move {
            run_proxy(listener, tracked_connector, task_cancel).await;
        });
        Ok(Self {
            address,
            blocked,
            cancel,
            task: Some(task),
        })
    }

    pub(super) fn url(&self) -> String {
        format!("http://{}", self.address)
    }

    pub(super) fn was_blocked(&self) -> bool {
        self.blocked.load(Ordering::SeqCst)
    }

    pub(super) async fn shutdown(mut self) {
        self.cancel.cancel();
        if let Some(mut task) = self.task.take() {
            if timeout(SHUTDOWN_TIMEOUT, &mut task).await.is_err() {
                task.abort();
                let _ = task.await;
            }
        }
    }
}

impl Drop for EgressProxy {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

fn public_connector() -> TargetConnector {
    Arc::new(|host, port| Box::pin(connect_public_target(host, port)))
}

async fn connect_public_target(
    host: String,
    port: u16,
) -> std::result::Result<TcpStream, ConnectError> {
    connect_public_target_with_resolver(host, port, |host, port| async move {
        let addresses = tokio::net::lookup_host((host.as_str(), port)).await?;
        Ok(addresses.collect())
    })
    .await
}

async fn connect_public_target_with_resolver<F, Fut>(
    host: String,
    port: u16,
    resolve: F,
) -> std::result::Result<TcpStream, ConnectError>
where
    F: FnOnce(String, u16) -> Fut,
    Fut: Future<Output = io::Result<Vec<SocketAddr>>>,
{
    let host = normalize_host(&host);
    if !is_allowed_port(port) || !is_allowed_host(&host) {
        return Err(ConnectError::Blocked);
    }

    let mut addresses = if let Ok(ip) = host.parse::<IpAddr>() {
        vec![SocketAddr::new(ip, port)]
    } else {
        match timeout(DNS_TIMEOUT, resolve(host.clone(), port)).await {
            Ok(Ok(addresses)) => addresses
                .into_iter()
                .map(|address| SocketAddr::new(address.ip(), port))
                .collect(),
            Ok(Err(error)) => return Err(ConnectError::Unavailable(error.to_string())),
            Err(_) => return Err(ConnectError::Unavailable("DNS resolution timed out".into())),
        }
    };
    addresses.sort_unstable();
    addresses.dedup();
    let ips = addresses.iter().map(SocketAddr::ip).collect::<Vec<_>>();
    if !resolved_ips_are_public(&ips) {
        return Err(ConnectError::Blocked);
    }

    let mut last_error = "target unavailable".to_string();
    let deadline = Instant::now() + CONNECT_TIMEOUT;
    for address in addresses.into_iter().take(MAX_TARGET_ADDRESSES) {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        match timeout(
            remaining.min(CONNECT_ATTEMPT_TIMEOUT),
            TcpStream::connect(address),
        )
        .await
        {
            Ok(Ok(stream)) => return Ok(stream),
            Ok(Err(error)) => last_error = error.to_string(),
            Err(_) => last_error = format!("connection to {address} timed out"),
        }
    }
    Err(ConnectError::Unavailable(last_error))
}

async fn run_proxy(listener: TcpListener, connector: TargetConnector, cancel: CancellationToken) {
    let mut connections = JoinSet::new();
    let connection_limit = Arc::new(Semaphore::new(MAX_CONNECTIONS));
    loop {
        tokio::select! {
            _ = cancel.cancelled() => break,
            accepted = listener.accept() => {
                let Ok((stream, peer)) = accepted else {
                    break;
                };
                if !peer.ip().is_loopback() {
                    continue;
                }
                let Ok(permit) = connection_limit.clone().try_acquire_owned() else {
                    continue;
                };
                let connector = connector.clone();
                let connection_cancel = cancel.clone();
                let connection_lease = Arc::new(permit);
                connections.spawn(async move {
                    if let Err(error) = serve_client(
                        stream,
                        connector,
                        connection_cancel,
                        connection_lease,
                    ).await {
                        tracing::debug!("egress proxy client closed: {error}");
                    }
                });
            }
            joined = connections.join_next(), if !connections.is_empty() => {
                if let Some(Err(error)) = joined {
                    tracing::debug!("egress proxy task failed: {error}");
                }
            }
        }
    }

    connections.abort_all();
    while connections.join_next().await.is_some() {}
}

async fn serve_client(
    stream: TcpStream,
    connector: TargetConnector,
    cancel: CancellationToken,
    connection_lease: Arc<OwnedSemaphorePermit>,
) -> Result<()> {
    let request_cancel = cancel.clone();
    let request_lease = connection_lease.clone();
    let service = service_fn(move |request| {
        proxy_request(
            request,
            connector.clone(),
            request_cancel.clone(),
            request_lease.clone(),
        )
    });
    let connection = server_http1::Builder::new()
        .serve_connection(TokioIo::new(stream), service)
        .with_upgrades();

    tokio::select! {
        _ = cancel.cancelled() => Ok(()),
        result = connection => result.context("serve egress proxy connection"),
    }
}

async fn proxy_request(
    request: Request<Incoming>,
    connector: TargetConnector,
    cancel: CancellationToken,
    connection_lease: Arc<OwnedSemaphorePermit>,
) -> std::result::Result<Response<ProxyBody>, Infallible> {
    let response = match Target::from_request(&request) {
        Ok(target) if request.method() == Method::CONNECT => {
            connect_tunnel(request, target, connector, cancel, connection_lease).await
        }
        Ok(target) => forward_http(request, target, connector, cancel).await,
        Err(message) => plain_response(StatusCode::BAD_REQUEST, message),
    };
    Ok(response)
}

#[derive(Debug)]
struct Target {
    host: String,
    port: u16,
    authority: String,
    origin_form: Uri,
}

impl Target {
    fn from_request(request: &Request<Incoming>) -> std::result::Result<Self, &'static str> {
        let is_connect = request.method() == Method::CONNECT;
        if !is_connect && !matches!(request.uri().scheme_str(), None | Some("http")) {
            return Err("unsupported proxy scheme");
        }

        let authority = request
            .uri()
            .authority()
            .cloned()
            .or_else(|| {
                is_connect
                    .then(|| request.uri().path().parse::<Authority>().ok())
                    .flatten()
            })
            .or_else(|| {
                request
                    .headers()
                    .get(HOST)
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.parse::<Authority>().ok())
            })
            .ok_or("missing proxy target")?;
        let port = authority
            .port_u16()
            .unwrap_or(if is_connect { 443 } else { 80 });
        let path = request
            .uri()
            .path_and_query()
            .map(|value| value.as_str())
            .unwrap_or("/");
        let origin_form = path.parse::<Uri>().map_err(|_| "invalid request path")?;

        Ok(Self {
            host: authority.host().to_owned(),
            port,
            authority: authority.as_str().to_owned(),
            origin_form,
        })
    }
}

async fn connect_tunnel(
    mut request: Request<Incoming>,
    target: Target,
    connector: TargetConnector,
    cancel: CancellationToken,
    connection_lease: Arc<OwnedSemaphorePermit>,
) -> Response<ProxyBody> {
    let mut upstream = match connect_target(&connector, &target).await {
        Ok(stream) => stream,
        Err(response) => return response,
    };
    let upgrade = hyper::upgrade::on(&mut request);
    tokio::spawn(async move {
        let _connection_lease = connection_lease;
        let Ok(upgraded) = upgrade.await else {
            return;
        };
        let mut client = TokioIo::new(upgraded);
        tokio::select! {
            _ = cancel.cancelled() => {}
            result = copy_bidirectional(&mut client, &mut upstream) => {
                if let Err(error) = result {
                    tracing::debug!("egress CONNECT tunnel closed: {error}");
                }
            }
        }
    });
    plain_response(StatusCode::OK, "")
}

async fn forward_http(
    mut request: Request<Incoming>,
    target: Target,
    connector: TargetConnector,
    cancel: CancellationToken,
) -> Response<ProxyBody> {
    let upstream = match connect_target(&connector, &target).await {
        Ok(stream) => stream,
        Err(response) => return response,
    };
    let (mut sender, connection) = match client_http1::handshake(TokioIo::new(upstream)).await {
        Ok(parts) => parts,
        Err(error) => {
            tracing::debug!("egress upstream handshake failed: {error}");
            return plain_response(StatusCode::BAD_GATEWAY, "upstream unavailable");
        }
    };
    tokio::spawn(async move {
        tokio::select! {
            _ = cancel.cancelled() => {}
            result = connection => {
                if let Err(error) = result {
                    tracing::debug!("egress upstream connection closed: {error}");
                }
            }
        }
    });

    *request.uri_mut() = target.origin_form;
    request.headers_mut().remove(PROXY_AUTHORIZATION);
    request.headers_mut().remove("proxy-connection");
    match HeaderValue::from_str(&target.authority) {
        Ok(host) => {
            request.headers_mut().insert(HOST, host);
        }
        Err(_) => return plain_response(StatusCode::BAD_REQUEST, "invalid proxy target"),
    }

    match sender.send_request(request).await {
        Ok(response) => {
            let (parts, body) = response.into_parts();
            Response::from_parts(parts, incoming_body(body))
        }
        Err(error) => {
            tracing::debug!("egress upstream request failed: {error}");
            plain_response(StatusCode::BAD_GATEWAY, "upstream unavailable")
        }
    }
}

async fn connect_target(
    connector: &TargetConnector,
    target: &Target,
) -> std::result::Result<TcpStream, Response<ProxyBody>> {
    match connector(target.host.clone(), target.port).await {
        Ok(stream) => Ok(stream),
        Err(ConnectError::Blocked) => {
            tracing::warn!(host = %target.host, port = target.port, "blocked yt-dlp egress target");
            Err(plain_response(block_status(), BLOCK_MESSAGE))
        }
        Err(ConnectError::Unavailable(error)) => {
            tracing::debug!(host = %target.host, port = target.port, "yt-dlp egress unavailable: {error}");
            Err(plain_response(
                StatusCode::BAD_GATEWAY,
                "upstream unavailable",
            ))
        }
    }
}

fn block_status() -> StatusCode {
    StatusCode::from_u16(BLOCK_STATUS_CODE).expect("valid private egress status")
}

fn plain_response(status: StatusCode, message: &str) -> Response<ProxyBody> {
    Response::builder()
        .status(status)
        .body(full_body(message))
        .expect("static proxy response")
}

fn full_body(message: &str) -> ProxyBody {
    Full::new(Bytes::copy_from_slice(message.as_bytes()))
        .map_err(|never| -> BoxError { match never {} })
        .boxed()
}

fn incoming_body(body: Incoming) -> ProxyBody {
    body.map_err(|error| -> BoxError { Box::new(error) })
        .boxed()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    fn connector_to(address: SocketAddr) -> TargetConnector {
        Arc::new(move |_host, _port| {
            Box::pin(async move {
                TcpStream::connect(address)
                    .await
                    .map_err(|error| ConnectError::Unavailable(error.to_string()))
            })
        })
    }

    async fn read_head(stream: &mut TcpStream) -> String {
        let mut bytes = Vec::new();
        let mut chunk = [0_u8; 512];
        while !bytes.windows(4).any(|window| window == b"\r\n\r\n") {
            let read = timeout(Duration::from_secs(2), stream.read(&mut chunk))
                .await
                .expect("proxy response timeout")
                .expect("read proxy response");
            assert!(read > 0, "proxy closed before response headers");
            bytes.extend_from_slice(&chunk[..read]);
        }
        String::from_utf8_lossy(&bytes).into_owned()
    }

    #[tokio::test]
    async fn production_proxy_blocks_private_connect_targets() {
        let proxy = EgressProxy::start().await.unwrap();
        let mut client = TcpStream::connect(proxy.address).await.unwrap();
        client
            .write_all(b"CONNECT 127.0.0.1:80 HTTP/1.1\r\nHost: 127.0.0.1:80\r\n\r\n")
            .await
            .unwrap();

        let response = read_head(&mut client).await;
        assert!(response.starts_with("HTTP/1.1 472"), "{response}");
        assert!(proxy.was_blocked());
        proxy.shutdown().await;
    }

    #[tokio::test]
    async fn dns_rebinding_is_revalidated_before_the_pinned_connect() {
        let calls = Arc::new(AtomicUsize::new(0));
        let initial_calls = calls.clone();
        super::super::net::validate_url_with_resolver(
            "http://video.example/start",
            move |_host, _port| async move {
                assert_eq!(initial_calls.fetch_add(1, Ordering::SeqCst), 0);
                Ok(vec!["93.184.216.34".parse().unwrap()])
            },
        )
        .await
        .unwrap();

        let sink = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let sink_address = sink.local_addr().unwrap();
        let rebound_calls = calls.clone();
        let result = connect_public_target_with_resolver(
            "video.example".into(),
            80,
            move |_host, _port| async move {
                assert_eq!(rebound_calls.fetch_add(1, Ordering::SeqCst), 1);
                Ok(vec![sink_address])
            },
        )
        .await;

        assert!(matches!(result, Err(ConnectError::Blocked)));
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert!(
            timeout(Duration::from_millis(150), sink.accept())
                .await
                .is_err(),
            "private rebound address received a connection"
        );
    }

    #[tokio::test]
    async fn http_proxy_rewrites_absolute_uri_and_host() {
        let target = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let target_address = target.local_addr().unwrap();
        let target_task = tokio::spawn(async move {
            let (mut stream, _) = target.accept().await.unwrap();
            let request = read_head(&mut stream).await;
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .await
                .unwrap();
            request
        });
        let proxy = EgressProxy::start_with_connector(connector_to(target_address))
            .await
            .unwrap();
        let mut client = TcpStream::connect(proxy.address).await.unwrap();
        client
            .write_all(
                b"GET http://video.example/clip?q=1 HTTP/1.1\r\nHost: wrong.example\r\nConnection: close\r\n\r\n",
            )
            .await
            .unwrap();
        let mut response = Vec::new();
        timeout(Duration::from_secs(2), client.read_to_end(&mut response))
            .await
            .expect("proxy response timeout")
            .unwrap();

        let forwarded = target_task.await.unwrap();
        assert!(
            forwarded.starts_with("GET /clip?q=1 HTTP/1.1"),
            "{forwarded}"
        );
        assert!(forwarded
            .to_ascii_lowercase()
            .contains("host: video.example"));
        assert!(String::from_utf8_lossy(&response).ends_with("ok"));
        proxy.shutdown().await;
    }

    #[tokio::test]
    async fn connect_proxy_tunnels_bytes_after_validation() {
        let target = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let target_address = target.local_addr().unwrap();
        let target_task = tokio::spawn(async move {
            let (mut stream, _) = target.accept().await.unwrap();
            let mut ping = [0_u8; 4];
            stream.read_exact(&mut ping).await.unwrap();
            assert_eq!(&ping, b"ping");
            stream.write_all(b"pong").await.unwrap();
        });
        let proxy = EgressProxy::start_with_connector(connector_to(target_address))
            .await
            .unwrap();
        let mut client = TcpStream::connect(proxy.address).await.unwrap();
        client
            .write_all(b"CONNECT video.example:443 HTTP/1.1\r\nHost: video.example:443\r\n\r\n")
            .await
            .unwrap();

        let response = read_head(&mut client).await;
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        client.write_all(b"ping").await.unwrap();
        let mut pong = [0_u8; 4];
        timeout(Duration::from_secs(2), client.read_exact(&mut pong))
            .await
            .expect("tunnel response timeout")
            .unwrap();
        assert_eq!(&pong, b"pong");

        target_task.await.unwrap();
        proxy.shutdown().await;
    }

    #[tokio::test]
    async fn ytdlp_redirect_to_private_target_is_blocked_before_connect() {
        match tokio::process::Command::new("yt-dlp")
            .arg("--version")
            .output()
            .await
        {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Err(error) => panic!("could not check yt-dlp: {error}"),
            Ok(output) => assert!(output.status.success(), "yt-dlp --version failed"),
        }

        let sink = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let sink_address = sink.local_addr().unwrap();
        let redirect = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await.unwrap();
        let redirect_address = redirect.local_addr().unwrap();
        let redirect_task = tokio::spawn(async move {
            let (mut stream, _) = redirect.accept().await.unwrap();
            let request = read_head(&mut stream).await;
            let response = format!(
                "HTTP/1.1 302 Found\r\nLocation: http://{sink_address}/secret\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
            );
            stream.write_all(response.as_bytes()).await.unwrap();
            request
        });
        let connector: TargetConnector = Arc::new(move |host, port| -> ConnectFuture {
            if normalize_host(&host) == "video.example" && port == 80 {
                Box::pin(async move {
                    TcpStream::connect(redirect_address)
                        .await
                        .map_err(|error| ConnectError::Unavailable(error.to_string()))
                })
            } else {
                Box::pin(connect_public_target(host, port))
            }
        });
        let proxy = EgressProxy::start_with_connector(connector).await.unwrap();
        let mut command = tokio::process::Command::new("yt-dlp");
        super::super::configure_ytdlp_network(&mut command, &proxy.url());
        command.args([
            "--simulate",
            "--no-playlist",
            "--no-warnings",
            "http://video.example/start",
        ]);
        let output = timeout(Duration::from_secs(15), command.output())
            .await
            .expect("yt-dlp redirect test timed out")
            .unwrap();

        assert!(
            !output.status.success(),
            "private redirect unexpectedly passed"
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("472"),
            "yt-dlp stderr did not preserve the egress policy status: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(proxy.was_blocked(), "proxy did not record its policy deny");
        let first_request = timeout(Duration::from_secs(2), redirect_task)
            .await
            .expect("yt-dlp did not reach the allowed initial host")
            .unwrap();
        assert!(first_request.starts_with("GET /start HTTP/1.1"));
        assert!(
            timeout(Duration::from_millis(150), sink.accept())
                .await
                .is_err(),
            "yt-dlp bypassed the proxy and reached the private redirect target"
        );
        proxy.shutdown().await;
    }
}

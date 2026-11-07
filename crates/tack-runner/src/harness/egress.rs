//! The runner's proxy for a local agent's traffic: one per run, on loopback.
//!
//! The profile chooses where a run may connect ([`EgressMode`]): `closed` is the agent's model
//! endpoint only, `listed` adds the hosts the profile names, `open` is any public host. In every
//! mode the proxy refuses loopback, private, link-local and cloud-metadata addresses, and it
//! checks the address a name resolved to, then connects to that address and no other, so a name
//! that answers differently the second time cannot reach an internal service.
//!
//! An HTTPS request arrives as `CONNECT host:port`: the proxy sees where the agent connects and
//! nothing of what it sends, so `open` limits the place, not the content; a public host the mode
//! allows can still receive data. A plain HTTP request is read, and only `GET`, `HEAD` and
//! `OPTIONS` to a URL of at most [`MAX_URL_BYTES`] pass.
//!
//! The runner sets `HTTP_PROXY`, `HTTPS_PROXY` and `NO_PROXY` for the agent (a request cannot:
//! see `env_policy`). Until the sandbox also closes every other way out, a program that ignores
//! the variables is not held by the proxy.

use std::{
    collections::HashSet,
    io,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use tack_protocol::execution::EgressMode;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::mpsc,
    task::{JoinHandle, JoinSet},
};

/// The longest URL a plain HTTP request may carry through the proxy.
pub const MAX_URL_BYTES: usize = 2048;
/// The longest request head (line and headers) the proxy reads before it refuses.
const MAX_HEAD_BYTES: usize = 16 * 1024;
const HEAD_TIMEOUT: Duration = Duration::from_secs(10);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// The refusals that wait for the engine before further ones are dropped.
const REFUSAL_BUFFER: usize = 64;
/// The methods a plain HTTP request may use: the ones that read.
const PLAIN_METHODS: [&str; 3] = ["GET", "HEAD", "OPTIONS"];

/// Names to addresses and addresses to connections: the two things the proxy asks of the
/// network, so a test can answer them without one.
#[async_trait]
pub trait Network: Send + Sync + 'static {
    /// Every address `host` resolves to.
    async fn resolve(&self, host: &str) -> io::Result<Vec<IpAddr>>;
    /// A connection to exactly `to`.
    async fn connect(&self, to: SocketAddr) -> io::Result<TcpStream>;
}

/// The machine's own resolver and sockets.
pub struct SystemNetwork;

#[async_trait]
impl Network for SystemNetwork {
    async fn resolve(&self, host: &str) -> io::Result<Vec<IpAddr>> {
        Ok(tokio::net::lookup_host((host, 0))
            .await?
            .map(|address| address.ip())
            .collect())
    }

    async fn connect(&self, to: SocketAddr) -> io::Result<TcpStream> {
        TcpStream::connect(to).await
    }
}

/// A destination the proxy refused, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkRefusal {
    pub host: String,
    pub port: u16,
    pub reason: String,
}

/// One entry of a host list: a name, or `*.name` for any host under it.
#[derive(Debug, Clone, PartialEq, Eq)]
enum HostRule {
    Exact(String),
    Under(String),
}

impl HostRule {
    fn parse(text: &str) -> Option<Self> {
        let text = text.trim().trim_end_matches('.').to_ascii_lowercase();
        if text.is_empty() {
            return None;
        }
        Some(match text.strip_prefix("*.") {
            Some(rest) if !rest.is_empty() => Self::Under(format!(".{rest}")),
            _ => Self::Exact(text),
        })
    }

    fn matches(&self, host: &str) -> bool {
        match self {
            Self::Exact(name) => host == name,
            Self::Under(suffix) => host.ends_with(suffix.as_str()),
        }
    }
}

/// Where one run may connect: the mode and the hosts that count for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EgressPolicy {
    mode: EgressMode,
    allowed: Vec<HostRule>,
}

impl EgressPolicy {
    /// `model_hosts` are the agent's model endpoints; `listed` is the profile's own list, which
    /// counts in `listed` mode only.
    pub fn new<'a>(
        mode: EgressMode,
        model_hosts: impl IntoIterator<Item = &'a str>,
        listed: impl IntoIterator<Item = &'a str>,
    ) -> Self {
        let listed = if mode == EgressMode::Listed {
            Some(listed)
        } else {
            None
        };
        Self {
            mode,
            allowed: model_hosts
                .into_iter()
                .chain(listed.into_iter().flatten())
                .filter_map(HostRule::parse)
                .collect(),
        }
    }

    pub fn mode(&self) -> EgressMode {
        self.mode
    }

    /// Whether the mode lets a run connect to this (lower-case) name at all.
    fn names(&self, host: &str) -> bool {
        self.mode == EgressMode::Open || self.allowed.iter().any(|rule| rule.matches(host))
    }
}

/// Whether the address is one no mode reaches: this machine, a private network, a link-local
/// address (the cloud metadata service `169.254.169.254` among them) or the unspecified,
/// multicast, shared-address and reserved ranges.
pub fn is_internal(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => internal_v4(ip),
        IpAddr::V6(ip) => internal_v6(ip),
    }
}

fn internal_v4(ip: Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();
    ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_multicast()
        // 0.0.0.0/8, shared address space 100.64.0.0/10 and the reserved 240.0.0.0/4
        || a == 0
        || (a == 100 && (64..128).contains(&b))
        || a >= 240
}

fn internal_v6(ip: Ipv6Addr) -> bool {
    if let Some(v4) = ip.to_ipv4_mapped() {
        return internal_v4(v4);
    }
    let segments = ip.segments();
    // 64:ff9b::/96 carries an IPv4 address in its last 32 bits
    if segments[..6] == [0x64, 0xff9b, 0, 0, 0, 0] {
        let [high, low] = [segments[6], segments[7]];
        return internal_v4(Ipv4Addr::new(
            (high >> 8) as u8,
            high as u8,
            (low >> 8) as u8,
            low as u8,
        ));
    }
    ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        // unique local fc00::/7 (the metadata address fd00:ec2::254 is in it), link-local fe80::/10
        || (segments[0] & 0xfe00) == 0xfc00
        || (segments[0] & 0xffc0) == 0xfe80
}

/// A running proxy. Dropping it stops the listener and every connection through it.
pub struct EgressProxy {
    address: SocketAddr,
    refusals: Option<mpsc::Receiver<NetworkRefusal>>,
    accepting: JoinHandle<()>,
}

impl Drop for EgressProxy {
    fn drop(&mut self) {
        // The connections are owned by the accepting task, so they end with it.
        self.accepting.abort();
    }
}

impl EgressProxy {
    /// Listens on a free loopback port.
    pub async fn start(policy: EgressPolicy) -> io::Result<Self> {
        Self::start_over(policy, Arc::new(SystemNetwork)).await
    }

    /// [`Self::start`] over a network of the caller's.
    pub async fn start_over(policy: EgressPolicy, network: Arc<dyn Network>) -> io::Result<Self> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
        let address = listener.local_addr()?;
        let (refused, refusals) = mpsc::channel(REFUSAL_BUFFER);
        let shared = Arc::new(Shared {
            policy,
            network,
            refused,
            told: Mutex::new(HashSet::new()),
        });
        let accepting = tokio::spawn(async move {
            let mut connections = JoinSet::new();
            loop {
                tokio::select! {
                    accepted = listener.accept() => match accepted {
                        Ok((stream, _)) => {
                            let shared = Arc::clone(&shared);
                            connections.spawn(async move { serve(stream, &shared).await });
                        }
                        Err(error) => {
                            tracing::warn!(%error, "the egress proxy could not accept a connection");
                            break;
                        }
                    },
                    Some(_) = connections.join_next(), if !connections.is_empty() => {}
                }
            }
        });
        Ok(Self {
            address,
            refusals: Some(refusals),
            accepting,
        })
    }

    /// Where the agent is told to send its traffic.
    pub fn address(&self) -> SocketAddr {
        self.address
    }

    /// The variables that send an agent's traffic here. Loopback is not sent through the proxy,
    /// which would refuse it.
    pub fn environment(&self) -> Vec<(String, String)> {
        let url = format!("http://{}", self.address);
        let no_proxy = "127.0.0.1,localhost,::1";
        ["HTTP_PROXY", "HTTPS_PROXY", "http_proxy", "https_proxy"]
            .into_iter()
            .map(|name| (name.to_owned(), url.clone()))
            .chain(
                ["NO_PROXY", "no_proxy"]
                    .into_iter()
                    .map(|name| (name.to_owned(), no_proxy.to_owned())),
            )
            .collect()
    }

    /// The destinations refused, one per destination and reason. Taken once.
    pub fn take_refusals(&mut self) -> Option<mpsc::Receiver<NetworkRefusal>> {
        self.refusals.take()
    }
}

struct Shared {
    policy: EgressPolicy,
    network: Arc<dyn Network>,
    refused: mpsc::Sender<NetworkRefusal>,
    /// The refusals already told, so a retrying agent is told once.
    told: Mutex<HashSet<(String, u16, String)>>,
}

impl Shared {
    fn refuse(&self, host: &str, port: u16, reason: String) {
        let first = self
            .told
            .lock()
            .map(|mut told| told.insert((host.to_owned(), port, reason.clone())))
            .unwrap_or(false);
        tracing::info!(host, port, reason, "egress refused");
        if first {
            let _ = self.refused.try_send(NetworkRefusal {
                host: host.to_owned(),
                port,
                reason,
            });
        }
    }

    /// The address to connect to for `host`, or why not.
    async fn admit(&self, host: &str) -> Result<IpAddr, String> {
        if let Ok(ip) = host.parse::<IpAddr>() {
            if is_internal(ip) {
                return Err(INTERNAL.to_owned());
            }
            return if self.policy.names(host) {
                Ok(ip)
            } else {
                Err(self.not_allowed())
            };
        }
        if !self.policy.names(host) {
            return Err(self.not_allowed());
        }
        let addresses = self
            .network
            .resolve(host)
            .await
            .map_err(|_| "does not resolve".to_owned())?;
        if addresses.is_empty() {
            return Err("does not resolve".to_owned());
        }
        if addresses.iter().copied().any(is_internal) {
            return Err(format!("resolves to an address that {INTERNAL}"));
        }
        Ok(addresses[0])
    }

    fn not_allowed(&self) -> String {
        match self.policy.mode {
            EgressMode::Closed => {
                "is not allowed: web access is closed, only the agent's model".to_owned()
            }
            EgressMode::Listed => "is not allowed: it is not on the profile's list".to_owned(),
            EgressMode::Open => "is not allowed".to_owned(),
        }
    }
}

const INTERNAL: &str = "is on this machine or a private, link-local or metadata network";

/// What the head of a request asked for.
enum Request {
    Tunnel {
        host: String,
        port: u16,
    },
    Plain {
        host: String,
        port: u16,
        /// The request line and headers as they go to the origin: origin-form target, one
        /// request per connection.
        head: Vec<u8>,
    },
}

async fn serve(mut client: TcpStream, shared: &Shared) {
    let (head, rest) = match read_head(&mut client).await {
        Ok(read) => read,
        Err(reply) => {
            let _ = client.write_all(reply.as_bytes()).await;
            return;
        }
    };
    let request = match parse(&head) {
        Ok(request) => request,
        Err(Refused { host, port, reason }) => {
            shared.refuse(&host, port, reason);
            let _ = client.write_all(FORBIDDEN.as_bytes()).await;
            return;
        }
    };
    let (host, port) = match &request {
        Request::Tunnel { host, port } | Request::Plain { host, port, .. } => {
            (host.as_str(), *port)
        }
    };
    let address = match shared.admit(host).await {
        Ok(ip) => SocketAddr::new(ip, port),
        Err(reason) => {
            shared.refuse(host, port, reason);
            let _ = client.write_all(FORBIDDEN.as_bytes()).await;
            return;
        }
    };
    let upstream = tokio::time::timeout(CONNECT_TIMEOUT, shared.network.connect(address)).await;
    let Ok(Ok(mut upstream)) = upstream else {
        let _ = client.write_all(BAD_GATEWAY.as_bytes()).await;
        return;
    };
    let sent_first = match &request {
        Request::Tunnel { .. } => client.write_all(ESTABLISHED.as_bytes()).await,
        Request::Plain { head, .. } => upstream.write_all(head).await,
    };
    if sent_first.is_err() || (!rest.is_empty() && upstream.write_all(&rest).await.is_err()) {
        return;
    }
    let carried = tokio::io::copy_bidirectional(&mut client, &mut upstream).await;
    let (up, down) = carried.unwrap_or((0, 0));
    tracing::info!(
        host,
        port,
        bytes_sent = up + rest.len() as u64,
        bytes_received = down,
        "egress reached"
    );
}

const FORBIDDEN: &str = "HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
const BAD_GATEWAY: &str =
    "HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
const BAD_REQUEST: &str =
    "HTTP/1.1 400 Bad Request\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
const ESTABLISHED: &str = "HTTP/1.1 200 Connection Established\r\n\r\n";

/// Reads up to the end of the request head. What came after it in the same reads is returned
/// too. An `Err` is the reply to send.
async fn read_head(client: &mut TcpStream) -> Result<(Vec<u8>, Vec<u8>), &'static str> {
    let mut buffer = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    let read = async {
        loop {
            if let Some(end) = find(&buffer, b"\r\n\r\n") {
                let rest = buffer.split_off(end + 4);
                return Ok((buffer, rest));
            }
            if buffer.len() > MAX_HEAD_BYTES {
                return Err(BAD_REQUEST);
            }
            match client.read(&mut chunk).await {
                Ok(0) | Err(_) => return Err(BAD_REQUEST),
                Ok(n) => buffer.extend_from_slice(&chunk[..n]),
            }
        }
    };
    tokio::time::timeout(HEAD_TIMEOUT, read)
        .await
        .unwrap_or(Err(BAD_REQUEST))
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

struct Refused {
    host: String,
    port: u16,
    reason: String,
}

fn refused(host: &str, port: u16, reason: impl Into<String>) -> Refused {
    Refused {
        host: host.to_owned(),
        port,
        reason: reason.into(),
    }
}

/// Reads the request line, and for a plain request the headers too.
fn parse(head: &[u8]) -> Result<Request, Refused> {
    let text = String::from_utf8_lossy(head);
    let mut lines = text.split("\r\n");
    let line = lines.next().unwrap_or_default();
    let mut words = line.split(' ');
    let (Some(method), Some(target), Some(version)) = (words.next(), words.next(), words.next())
    else {
        return Err(refused(line, 0, "was not an HTTP proxy request"));
    };
    if !version.starts_with("HTTP/1.") || words.next().is_some() {
        return Err(refused(line, 0, "was not an HTTP proxy request"));
    }
    if method.eq_ignore_ascii_case("CONNECT") {
        let (host, port) = authority(target, 443)
            .ok_or_else(|| refused(target, 0, "was not a host:port to connect to"))?;
        return Ok(Request::Tunnel { host, port });
    }
    let upper = method.to_ascii_uppercase();
    let rest = target
        .get(..7)
        .filter(|scheme| scheme.eq_ignore_ascii_case("http://"))
        .map(|_| &target[7..])
        .ok_or_else(|| refused(target, 0, "was not an absolute http:// URL, the only plain request that goes through the proxy"))?;
    let (authority_text, path) = match rest.find(['/', '?']) {
        Some(at) => rest.split_at(at),
        None => (rest, "/"),
    };
    let path = if path.starts_with('?') {
        format!("/{path}")
    } else {
        path.to_owned()
    };
    let (host, port) = authority(authority_text, 80)
        .ok_or_else(|| refused(authority_text, 0, "was not a valid host"))?;
    if !PLAIN_METHODS.contains(&upper.as_str()) {
        return Err(refused(
            &host,
            port,
            format!("was asked with {upper}; only GET, HEAD and OPTIONS pass over plain HTTP"),
        ));
    }
    if target.len() > MAX_URL_BYTES {
        return Err(refused(
            &host,
            port,
            format!("was asked with a URL longer than {MAX_URL_BYTES} bytes"),
        ));
    }
    let mut out = format!("{upper} {path} {version}\r\n");
    let mut has_host = false;
    for header in lines.filter(|header| !header.is_empty()) {
        let name = header
            .split(':')
            .next()
            .unwrap_or_default()
            .trim()
            .to_ascii_lowercase();
        if matches!(
            name.as_str(),
            "connection" | "proxy-connection" | "proxy-authorization" | "keep-alive" | "upgrade"
        ) {
            continue;
        }
        has_host |= name == "host";
        out.push_str(header);
        out.push_str("\r\n");
    }
    if !has_host {
        out.push_str(&format!("Host: {authority_text}\r\n"));
    }
    out.push_str("Connection: close\r\n\r\n");
    Ok(Request::Plain {
        host,
        port,
        head: out.into_bytes(),
    })
}

/// `host`, `host:port` or `[v6]:port`, the host in lower case without a trailing dot.
fn authority(text: &str, default_port: u16) -> Option<(String, u16)> {
    let text = text.rsplit('@').next().unwrap_or(text);
    let (host, port) = if let Some(rest) = text.strip_prefix('[') {
        let (host, after) = rest.split_once(']')?;
        let port = match after.strip_prefix(':') {
            Some(port) => port.parse().ok()?,
            None if after.is_empty() => default_port,
            None => return None,
        };
        (host, port)
    } else {
        match text.rsplit_once(':') {
            Some((host, port)) => (host, port.parse().ok()?),
            None => (text, default_port),
        }
    };
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    (!host.is_empty() && port != 0).then_some((host, port))
}

#[cfg(test)]
#[path = "egress/tests.rs"]
mod tests;

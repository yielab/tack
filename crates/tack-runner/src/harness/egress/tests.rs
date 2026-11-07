use std::{collections::HashMap, net::IpAddr, sync::Mutex};

use super::*;

/// Names and sockets without a network: a name resolves to what the test says, and a connection
/// to any address reaches the local echo server, with the address it was asked for recorded.
struct Stub {
    names: HashMap<String, Vec<IpAddr>>,
    upstream: SocketAddr,
    resolved: Mutex<Vec<String>>,
    connected: Mutex<Vec<SocketAddr>>,
}

#[async_trait]
impl Network for Stub {
    async fn resolve(&self, host: &str) -> io::Result<Vec<IpAddr>> {
        self.resolved.lock().unwrap().push(host.to_owned());
        self.names
            .get(host)
            .cloned()
            .ok_or_else(|| io::Error::other("unknown name"))
    }

    async fn connect(&self, to: SocketAddr) -> io::Result<TcpStream> {
        self.connected.lock().unwrap().push(to);
        TcpStream::connect(self.upstream).await
    }
}

/// An upstream that answers every request with `pong` and then echoes what it reads.
async fn echo() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            tokio::spawn(async move {
                let mut buffer = [0u8; 4096];
                loop {
                    match stream.read(&mut buffer).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => {
                            if stream.write_all(&buffer[..n]).await.is_err() {
                                return;
                            }
                        }
                    }
                }
            });
        }
    });
    address
}

struct Run {
    proxy: EgressProxy,
    network: Arc<Stub>,
}

async fn run(mode: EgressMode, models: &[&str], listed: &[&str]) -> Run {
    let network = Arc::new(Stub {
        names: HashMap::from([
            (
                "api.model.test".to_owned(),
                vec!["203.0.113.10".parse().unwrap()],
            ),
            (
                "docs.listed.test".to_owned(),
                vec!["203.0.113.20".parse().unwrap()],
            ),
            (
                "www.listed.test".to_owned(),
                vec!["203.0.113.21".parse().unwrap()],
            ),
            (
                "blog.public.test".to_owned(),
                vec!["203.0.113.30".parse().unwrap()],
            ),
            ("inside.test".to_owned(), vec!["10.0.0.9".parse().unwrap()]),
            (
                "split.test".to_owned(),
                vec![
                    "203.0.113.40".parse().unwrap(),
                    "169.254.169.254".parse().unwrap(),
                ],
            ),
            (
                "meta.test".to_owned(),
                vec!["fd00:ec2::254".parse().unwrap()],
            ),
        ]),
        upstream: echo().await,
        resolved: Mutex::new(Vec::new()),
        connected: Mutex::new(Vec::new()),
    });
    let proxy = EgressProxy::start_over(
        EgressPolicy::new(mode, models.iter().copied(), listed.iter().copied()),
        network.clone(),
        false,
    )
    .await
    .unwrap();
    Run { proxy, network }
}

/// What the proxy answers to `request`, up to the blank line, and what the tunnel then echoes.
async fn ask(run: &Run, request: &str) -> (String, String) {
    let mut stream = TcpStream::connect(run.proxy.address()).await.unwrap();
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut head = Vec::new();
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        if stream.read(&mut byte).await.unwrap_or(0) == 0 {
            break;
        }
        head.push(byte[0]);
    }
    let head = String::from_utf8(head).unwrap();
    let mut echoed = String::new();
    if head.contains(" 200 ") {
        stream.write_all(b"ping").await.unwrap();
        let mut back = [0u8; 4];
        stream.read_exact(&mut back).await.unwrap();
        echoed = String::from_utf8(back.to_vec()).unwrap();
    }
    (head, echoed)
}

fn connect(target: &str) -> String {
    format!("CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n\r\n")
}

fn status(head: &str) -> &str {
    head.split("\r\n").next().unwrap()
}

#[test]
fn an_address_on_this_machine_or_a_private_network_is_internal_and_a_public_one_is_not() {
    for internal in [
        "127.0.0.1",
        "127.8.8.8",
        "10.0.0.1",
        "172.16.5.5",
        "172.31.255.255",
        "192.168.1.1",
        "169.254.169.254",
        "169.254.0.1",
        "0.0.0.0",
        "100.64.0.1",
        "224.0.0.1",
        "255.255.255.255",
        "::1",
        "::",
        "fd00:ec2::254",
        "fc00::1",
        "fe80::1",
        "::ffff:127.0.0.1",
        "::ffff:10.1.2.3",
        "64:ff9b::a00:1",
        "ff02::1",
    ] {
        assert!(is_internal(internal.parse().unwrap()), "{internal}");
    }
    for public in [
        "8.8.8.8",
        "203.0.113.7",
        "172.32.0.1",
        "100.63.0.1",
        "100.128.0.1",
        "2606:4700:4700::1111",
        "::ffff:8.8.8.8",
    ] {
        assert!(!is_internal(public.parse().unwrap()), "{public}");
    }
}

#[test]
fn a_host_rule_is_a_name_or_everything_under_one_name() {
    let policy = EgressPolicy::new(
        EgressMode::Listed,
        ["API.model.test."],
        ["*.listed.test", "exact.test"],
    );
    for name in [
        "api.model.test",
        "docs.listed.test",
        "a.b.listed.test",
        "exact.test",
    ] {
        assert!(policy.names(name), "{name}");
    }
    for name in [
        "listed.test",
        "evil-listed.test",
        "exact.test.evil.test",
        "other.test",
    ] {
        assert!(!policy.names(name), "{name}");
    }
}

#[test]
fn the_profiles_hosts_count_only_in_listed_mode() {
    let hosts = ["docs.listed.test"];
    assert!(EgressPolicy::new(EgressMode::Listed, [], hosts).names("docs.listed.test"));
    assert!(!EgressPolicy::new(EgressMode::Closed, [], hosts).names("docs.listed.test"));
}

#[tokio::test]
async fn closed_reaches_the_model_and_refuses_a_public_host_that_is_not_listed() {
    let run = run(
        EgressMode::Closed,
        &["api.model.test"],
        &["docs.listed.test"],
    )
    .await;
    let (head, echoed) = ask(&run, &connect("api.model.test:443")).await;
    assert!(status(&head).contains("200"), "{head}");
    assert_eq!(echoed, "ping");
    assert_eq!(
        *run.network.connected.lock().unwrap(),
        ["203.0.113.10:443".parse::<SocketAddr>().unwrap()]
    );

    for host in ["blog.public.test", "docs.listed.test"] {
        let (head, _) = ask(&run, &connect(&format!("{host}:443"))).await;
        assert!(status(&head).contains("403"), "{host}: {head}");
    }
    assert_eq!(
        run.network.connected.lock().unwrap().len(),
        1,
        "nothing else was dialled"
    );
}

#[tokio::test]
async fn listed_reaches_the_model_and_the_listed_hosts_and_nothing_else() {
    let mut run = run(EgressMode::Listed, &["api.model.test"], &["*.listed.test"]).await;
    for host in ["api.model.test", "docs.listed.test", "www.listed.test"] {
        let (head, echoed) = ask(&run, &connect(&format!("{host}:443"))).await;
        assert!(status(&head).contains("200"), "{host}: {head}");
        assert_eq!(echoed, "ping");
    }
    let (head, _) = ask(&run, &connect("blog.public.test:443")).await;
    assert!(status(&head).contains("403"), "{head}");
    let mut refusals = run.proxy.take_refusals().unwrap();
    let refusal = refusals.recv().await.unwrap();
    assert_eq!(refusal.host, "blog.public.test");
    assert_eq!(refusal.port, 443);
    assert!(
        refusal.reason.contains("not on the profile's list"),
        "{refusal:?}"
    );
}

#[tokio::test]
async fn open_reaches_any_public_host_and_still_refuses_internal_ones() {
    let mut run = run(EgressMode::Open, &["api.model.test"], &[]).await;
    let (head, echoed) = ask(&run, &connect("blog.public.test:443")).await;
    assert!(status(&head).contains("200"), "{head}");
    assert_eq!(echoed, "ping");
    let dialled = run.network.connected.lock().unwrap().len();

    for target in [
        "127.0.0.1:8080",
        "10.0.0.5:443",
        "169.254.169.254:80",
        "[fd00:ec2::254]:80",
        "[::1]:3210",
        // a name that resolves to a private address, one that resolves to a public address and
        // a private one, and one that resolves to the IPv6 metadata address
        "inside.test:443",
        "split.test:443",
        "meta.test:443",
    ] {
        let (head, _) = ask(&run, &connect(target)).await;
        assert!(status(&head).contains("403"), "{target}: {head}");
    }
    assert_eq!(
        run.network.connected.lock().unwrap().len(),
        dialled,
        "no refused destination was dialled"
    );
    let mut refusals = run.proxy.take_refusals().unwrap();
    let mut told = Vec::new();
    while let Ok(refusal) = refusals.try_recv() {
        told.push(refusal.host);
    }
    for host in [
        "127.0.0.1",
        "10.0.0.5",
        "169.254.169.254",
        "fd00:ec2::254",
        "inside.test",
    ] {
        assert!(told.iter().any(|t| t == host), "{host} in {told:?}");
    }
}

#[tokio::test]
async fn a_name_is_resolved_once_and_the_connection_goes_to_that_address() {
    let run = run(EgressMode::Open, &[], &[]).await;
    ask(&run, &connect("blog.public.test:443")).await;
    assert_eq!(*run.network.resolved.lock().unwrap(), ["blog.public.test"]);
    assert_eq!(
        *run.network.connected.lock().unwrap(),
        ["203.0.113.30:443".parse::<SocketAddr>().unwrap()],
        "the address that was checked is the address that was dialled"
    );
}

#[tokio::test]
async fn a_refused_destination_is_told_once_however_often_the_agent_retries() {
    let mut run = run(EgressMode::Closed, &[], &[]).await;
    for _ in 0..3 {
        ask(&run, &connect("blog.public.test:443")).await;
    }
    ask(&run, &connect("blog.public.test:8443")).await;
    let mut refusals = run.proxy.take_refusals().unwrap();
    let mut ports = Vec::new();
    while let Ok(refusal) = refusals.try_recv() {
        ports.push(refusal.port);
    }
    assert_eq!(ports, [443, 8443]);
}

#[tokio::test]
async fn plain_http_passes_a_read_and_refuses_a_write_and_a_long_url() {
    let mut run = run(EgressMode::Open, &[], &[]).await;
    let get = "GET http://blog.public.test/page?a=1 HTTP/1.1\r\nHost: blog.public.test\r\n\
               Proxy-Connection: keep-alive\r\nConnection: keep-alive\r\nAccept: */*\r\n\r\n";
    let mut stream = TcpStream::connect(run.proxy.address()).await.unwrap();
    stream.write_all(get.as_bytes()).await.unwrap();
    // The echo server sends the head the proxy forwarded straight back.
    let mut forwarded = vec![0u8; 512];
    let n = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut forwarded))
        .await
        .unwrap()
        .unwrap();
    let forwarded = String::from_utf8_lossy(&forwarded[..n]).into_owned();
    assert!(
        forwarded.starts_with("GET /page?a=1 HTTP/1.1\r\n"),
        "{forwarded}"
    );
    assert!(forwarded.contains("Connection: close\r\n"), "{forwarded}");
    assert!(!forwarded.contains("keep-alive"), "{forwarded}");
    assert!(forwarded.contains("Accept: */*"), "{forwarded}");

    let (head, _) = ask(
        &run,
        "POST http://blog.public.test/upload HTTP/1.1\r\nHost: blog.public.test\r\nContent-Length: 0\r\n\r\n",
    )
    .await;
    assert!(status(&head).contains("403"), "{head}");
    let long = format!(
        "GET http://blog.public.test/{} HTTP/1.1\r\nHost: blog.public.test\r\n\r\n",
        "a".repeat(MAX_URL_BYTES)
    );
    let (head, _) = ask(&run, &long).await;
    assert!(status(&head).contains("403"), "{head}");

    let mut refusals = run.proxy.take_refusals().unwrap();
    let post = refusals.recv().await.unwrap();
    assert!(post.reason.contains("asked with POST"), "{post:?}");
    let url = refusals.recv().await.unwrap();
    assert!(url.reason.contains("longer than"), "{url:?}");
}

#[tokio::test]
async fn plain_http_to_an_internal_address_is_refused_like_a_tunnel() {
    let run = run(EgressMode::Open, &[], &[]).await;
    let (head, _) = ask(
        &run,
        "GET http://169.254.169.254/latest/meta-data/ HTTP/1.1\r\nHost: 169.254.169.254\r\n\r\n",
    )
    .await;
    assert!(status(&head).contains("403"), "{head}");
    assert!(run.network.connected.lock().unwrap().is_empty());
}

#[tokio::test]
async fn something_that_is_not_a_proxy_request_is_refused() {
    let run = run(EgressMode::Open, &[], &[]).await;
    for request in [
        "GET /relative HTTP/1.1\r\nHost: x\r\n\r\n",
        "nonsense\r\n\r\n",
    ] {
        let (head, _) = ask(&run, request).await;
        assert!(status(&head).contains("403"), "{request}: {head}");
    }
}

#[tokio::test]
async fn the_environment_sends_traffic_here_and_keeps_loopback_off_the_proxy() {
    let run = run(EgressMode::Closed, &[], &[]).await;
    let env: HashMap<_, _> = run.proxy.environment().into_iter().collect();
    let url = format!("http://{}", run.proxy.address());
    for name in ["HTTP_PROXY", "HTTPS_PROXY", "http_proxy", "https_proxy"] {
        assert_eq!(env[name], url, "{name}");
    }
    assert!(env["NO_PROXY"].contains("127.0.0.1") && env["NO_PROXY"].contains("localhost"));
    assert_eq!(env["no_proxy"], env["NO_PROXY"]);
}

#[tokio::test]
async fn dropping_the_proxy_closes_the_port() {
    let run = run(EgressMode::Open, &[], &[]).await;
    let address = run.proxy.address();
    drop(run);
    for _ in 0..50 {
        if TcpStream::connect(address).await.is_err() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("the port still accepts after the proxy was dropped");
}

#[cfg(unix)]
#[tokio::test]
async fn the_unix_socket_is_held_to_the_same_policy_and_goes_away_with_the_proxy() {
    let mut run = run(EgressMode::Closed, &["api.model.test"], &[]).await;
    // `run` starts the proxy with no socket; start one that has it over the same kind of network.
    let network = run.network.clone();
    run.proxy = EgressProxy::start_over(
        EgressPolicy::new(EgressMode::Closed, ["api.model.test"], []),
        network,
        true,
    )
    .await
    .unwrap();
    let directory = run
        .proxy
        .socket_dir()
        .expect("a socket directory")
        .to_path_buf();
    let socket = directory.join(crate::harness::sandbox::BRIDGE_SOCKET_NAME);
    let asked = |target: &'static str| {
        let socket = socket.clone();
        async move {
            let mut stream = tokio::net::UnixStream::connect(socket).await.unwrap();
            stream.write_all(connect(target).as_bytes()).await.unwrap();
            let mut head = [0u8; 12];
            stream.read_exact(&mut head).await.unwrap();
            String::from_utf8(head.to_vec()).unwrap()
        }
    };
    assert_eq!(asked("api.model.test:443").await, "HTTP/1.1 200");
    assert_eq!(asked("blog.public.test:443").await, "HTTP/1.1 403");
    assert_eq!(asked("10.0.0.9:80").await, "HTTP/1.1 403");
    drop(run);
    assert!(!directory.exists(), "the socket directory was left behind");
}

#[test]
fn only_the_sandboxes_own_start_makes_the_executable_a_forwarder() {
    let args = |parts: &[&str]| {
        parts
            .iter()
            .map(|part| (*part).to_owned())
            .collect::<Vec<_>>()
    };
    assert!(bridge::run_if_requested(args(&["tack", "serve"])).is_none());
    assert!(bridge::run_if_requested(args(&["tack"])).is_none());
    assert!(bridge::run_if_requested(args(&["tack", "__tack-egress-bridge"])).is_none());
    // Asked to forward and given nothing to run: it ends in failure rather than serving.
    assert_eq!(
        bridge::run_if_requested(args(&["tack", "__tack-egress-bridge", "--"])),
        Some(1)
    );
}

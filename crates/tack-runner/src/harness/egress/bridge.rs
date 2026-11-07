//! The forwarder that gives an agent in a sandbox without a network the runner's proxy.
//!
//! The sandbox is started with the runner's own executable as `<exe> __tack-egress-bridge --
//! <agent> <args>…`. This process listens on the sandbox's loopback, forwards every connection to
//! the proxy's Unix socket (which the sandbox shows), and runs the agent as its child, so
//! `HTTP_PROXY=http://127.0.0.1:<port>` reaches the proxy the same as it does outside. It keeps
//! the agent's exit status. The sandbox ends with it: nothing the agent starts outlives the
//! forwarder.

use std::{
    io,
    net::{Ipv4Addr, TcpListener, TcpStream},
    path::Path,
    process::Command,
    thread,
};

use crate::harness::sandbox::{BRIDGE_ARG, BRIDGE_PORT, BRIDGE_SOCKET_DIR, BRIDGE_SOCKET_NAME};

/// When this process was started as the sandbox's forwarder (`args` are its arguments, the
/// program's name first), runs it and returns the exit code to end with; `None` for any other
/// start.
pub fn run_if_requested(args: impl IntoIterator<Item = String>) -> Option<u8> {
    let mut args = args.into_iter().skip(1);
    if args.next().as_deref() != Some(BRIDGE_ARG) || args.next().as_deref() != Some("--") {
        return None;
    }
    let Some(program) = args.next() else {
        eprintln!("tack: the egress bridge was given no program to run");
        return Some(1);
    };
    let socket = Path::new(BRIDGE_SOCKET_DIR).join(BRIDGE_SOCKET_NAME);
    Some(match run(&program, &args.collect::<Vec<_>>(), &socket) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("tack: the egress bridge could not start `{program}`: {error}");
            1
        }
    })
}

fn run(program: &str, args: &[String], socket: &Path) -> io::Result<u8> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, BRIDGE_PORT))?;
    let socket = socket.to_path_buf();
    thread::spawn(move || {
        for client in listener.incoming().flatten() {
            let socket = socket.clone();
            thread::spawn(move || forward(client, &socket));
        }
    });
    let status = Command::new(program).args(args).status()?;
    Ok(exit_code(status))
}

/// How the agent ended, as this process ends: its code, or 128 and the signal that stopped it.
fn exit_code(status: std::process::ExitStatus) -> u8 {
    #[cfg(unix)]
    if let Some(signal) = std::os::unix::process::ExitStatusExt::signal(&status) {
        return 128u8.saturating_add(signal as u8);
    }
    status.code().map_or(1, |code| code as u8)
}

/// Carries one connection both ways until either side closes.
#[cfg(unix)]
fn forward(client: TcpStream, socket: &Path) {
    let Ok(proxy) = std::os::unix::net::UnixStream::connect(socket) else {
        return;
    };
    let (Ok(mut from_client), Ok(mut to_client)) = (client.try_clone(), client.try_clone()) else {
        return;
    };
    let (Ok(mut from_proxy), Ok(mut to_proxy)) = (proxy.try_clone(), proxy.try_clone()) else {
        return;
    };
    let upstream = thread::spawn(move || {
        let _ = io::copy(&mut from_client, &mut to_proxy);
        let _ = to_proxy.shutdown(std::net::Shutdown::Write);
    });
    let _ = io::copy(&mut from_proxy, &mut to_client);
    let _ = to_client.shutdown(std::net::Shutdown::Write);
    let _ = upstream.join();
}

/// There are no Unix sockets here, so no sandbox to forward out of.
#[cfg(not(unix))]
fn forward(_: TcpStream, _: &Path) {}

//! Relay -> direct path upgrade under continuous traffic.
//!
//! Field report on #10906 (via discussion #11024): on same-NAT/hairpin
//! topologies, a comparable overlay stack saw the relay path work and then a
//! direct-upgrade rekey desync silently drop queued messages until restart.
//! This test pins the equivalent seam in roam: two nodes meet through a relay
//! (an in-process iroh test relay), the client dials with a relay-only
//! address (exactly what a browser card connect does), traffic flows, iroh
//! holepunches a direct localhost path mid-stream, and every frame sent
//! before, during, and after the migration must come back intact and in
//! order.
//!
//! Localhost holepunching is the closest CI-runnable stand-in for the
//! same-NAT hairpin case: both sides observe reflexive addresses that end up
//! on the loopback/LAN, and the upgrade + rekey machinery is the same code
//! path that runs behind a hairpinning NAT.

use std::sync::Arc;

use futures::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use goose_roaming::{
    AcpStreamServer, Directory, RelayEntry, RelaySettings, RoamingConfig, RoamingIdentity,
    RoamingNode, TrustBook,
};
use iroh::{EndpointId, TransportAddr};

/// Echoes every byte back until the client closes its send side. Unlike the
/// one-shot echo in `end_to_end.rs`, this keeps the duplex busy across the
/// path migration so a rekey desync would surface as lost or corrupted data.
#[derive(Debug)]
struct StreamingEchoServer;

impl AcpStreamServer for StreamingEchoServer {
    fn serve_stream(
        &self,
        _client: EndpointId,
        mut recv: Box<dyn AsyncRead + Send + Unpin>,
        mut send: Box<dyn AsyncWrite + Send + Unpin>,
    ) -> futures::future::BoxFuture<'static, anyhow::Result<()>> {
        Box::pin(async move {
            let mut buf = [0u8; 4096];
            loop {
                let n = recv.read(&mut buf).await?;
                if n == 0 {
                    return Ok(());
                }
                send.write_all(&buf[..n]).await?;
                send.flush().await?;
            }
        })
    }

    fn agent_id(&self) -> String {
        "streaming-echo".to_string()
    }
}

async fn bind_node_with_relay(relay: RelaySettings) -> Arc<RoamingNode> {
    RoamingNode::bind(RoamingConfig {
        identity: RoamingIdentity::generate(),
        relay,
        trust: TrustBook::new(),
        trust_path: None,
        directory: Directory::new(),
        bind_addr: None,
        relay_tls: Some(iroh::tls::CaTlsConfig::insecure_skip_verify()),
    })
    .await
    .expect("bind node")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn relay_to_direct_upgrade_loses_no_data() {
    let (_relay_map, relay_url, _relay_guard) = iroh::test_utils::run_relay_server()
        .await
        .expect("run test relay");
    let relay = RelaySettings::Custom(vec![RelayEntry::new(relay_url.to_string())]);

    // Holepunching races the roam handshake inside `connect_with_addr`, so
    // on a slow machine the connection can already be direct when it is
    // returned. Such a run never migrates under traffic, so start over with
    // fresh nodes that have no remembered direct path.
    let mut attempt = 1;
    let (host, _client, mut stream) = loop {
        let (host, client, stream) = connect_relay_only(&relay).await;
        if matches!(
            selected_remote(&stream.conn.paths()),
            Some(TransportAddr::Relay(_))
        ) {
            break (host, client, stream);
        }
        host.shutdown().await.unwrap();
        assert!(
            attempt < 5,
            "direct path was selected before connect returned in {attempt} attempts"
        );
        attempt += 1;
    };
    let conn = stream.conn.clone();
    let mut paths = conn.paths_stream();

    // Phase 1: traffic while on the relay path.
    let mut counter: u64 = 0;
    exchange_frames(&mut stream, &mut counter, 50).await;

    // Wait for a direct (IP) path to be selected, pumping traffic the whole
    // time so the migration happens under load. `paths_stream` starts with
    // the current snapshot, so an upgrade during phase 1 is not missed.
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    let mut direct_selected = false;
    while !direct_selected {
        tokio::select! {
            list = futures::StreamExt::next(&mut paths) => {
                let list = list.expect("connection closed before direct upgrade");
                direct_selected = matches!(selected_remote(&list), Some(TransportAddr::Ip(_)));
            }
            _ = tokio::time::sleep(std::time::Duration::from_millis(50)) => {
                exchange_frames(&mut stream, &mut counter, 5).await;
            }
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "no direct path was selected within 30s (holepunching failed on localhost)"
        );
    }

    // Phase 2: the rekey/migration just happened under load. Everything must
    // still round-trip exactly.
    exchange_frames(&mut stream, &mut counter, 50).await;

    assert!(counter >= 100, "test exchanged {counter} frames");
    stream.send.finish().unwrap();
    host.shutdown().await.unwrap();
}

/// Bind a fresh host and client on `relay` and dial the host with a
/// relay-only address, the browser-card shape: the direct path must be found
/// by holepunching, not seeded by the dialer.
async fn connect_relay_only(
    relay: &RelaySettings,
) -> (
    Arc<RoamingNode>,
    Arc<RoamingNode>,
    goose_roaming::RoamingClientStream,
) {
    let host = bind_node_with_relay(relay.clone()).await;
    host.share(Arc::new(StreamingEchoServer))
        .await
        .expect("share");

    let client = bind_node_with_relay(relay.clone()).await;
    host.trust().lock().await.accept(&client.endpoint_id());

    assert!(
        host.wait_online(std::time::Duration::from_secs(15)).await,
        "host never reached the test relay"
    );
    assert!(
        client.wait_online(std::time::Duration::from_secs(15)).await,
        "client never reached the test relay"
    );

    let mut addr = iroh::EndpointAddr::new(host.endpoint_id());
    addr.addrs.insert(TransportAddr::Relay(
        relay_url_of(&host).expect("host has a relay addr"),
    ));
    let stream = client
        .connect_with_addr(addr, Some("hairpin-test".into()))
        .await
        .expect("connect through relay");
    (host, client, stream)
}

fn selected_remote(paths: &iroh::endpoint::PathList) -> Option<TransportAddr> {
    paths
        .iter()
        .find(|p| p.is_selected())
        .map(|p| p.remote_addr().clone())
}

/// Send `n` numbered frames and require each to echo back verbatim, in order.
/// Any dropped or reordered frame during path migration fails loudly here.
async fn exchange_frames(
    stream: &mut goose_roaming::RoamingClientStream,
    counter: &mut u64,
    n: usize,
) {
    for _ in 0..n {
        let msg = format!("frame-{:08}", *counter);
        stream
            .send
            .write_all(msg.as_bytes())
            .await
            .unwrap_or_else(|e| panic!("write failed at frame {counter}: {e}"));
        let mut buf = vec![0u8; msg.len()];
        tokio::time::timeout(
            std::time::Duration::from_secs(10),
            stream.recv.read_exact(&mut buf),
        )
        .await
        .unwrap_or_else(|_| panic!("echo timed out at frame {counter} — data lost in migration"))
        .unwrap_or_else(|e| panic!("read failed at frame {counter}: {e}"));
        assert_eq!(
            buf,
            msg.as_bytes(),
            "frame {counter} corrupted across path migration"
        );
        *counter += 1;
    }
}

/// Burst of concurrent dials to one host (field report: a comparable stack
/// lost replies under parallel opens until dials were serialized per
/// process). Roam multiplexes streams over one QUIC connection per peer
/// pair, so parallel connects must all succeed and each stream must echo
/// independently.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_dial_burst() {
    let (_relay_map, relay_url, _relay_guard) = iroh::test_utils::run_relay_server()
        .await
        .expect("run test relay");
    let relay = RelaySettings::Custom(vec![RelayEntry::new(relay_url.to_string())]);

    let host = bind_node_with_relay(relay.clone()).await;
    host.share(Arc::new(StreamingEchoServer))
        .await
        .expect("share");
    assert!(host.wait_online(std::time::Duration::from_secs(15)).await);

    let client = bind_node_with_relay(relay).await;
    host.trust().lock().await.accept(&client.endpoint_id());
    assert!(client.wait_online(std::time::Duration::from_secs(15)).await);

    let addr = {
        let mut a = iroh::EndpointAddr::new(host.endpoint_id());
        a.addrs.insert(TransportAddr::Relay(
            relay_url_of(&host).expect("host has a relay addr"),
        ));
        a
    };

    let mut tasks = Vec::new();
    for i in 0..8u32 {
        let client = client.clone();
        let addr = addr.clone();
        tasks.push(tokio::spawn(async move {
            let mut stream = client
                .connect_with_addr(addr, Some(format!("burst-{i}")))
                .await
                .unwrap_or_else(|e| panic!("parallel dial {i} failed: {e}"));
            let msg = format!("burst-payload-{i:04}");
            stream.send.write_all(msg.as_bytes()).await.unwrap();
            let mut buf = vec![0u8; msg.len()];
            tokio::time::timeout(
                std::time::Duration::from_secs(10),
                stream.recv.read_exact(&mut buf),
            )
            .await
            .unwrap_or_else(|_| panic!("dial {i}: echo timed out under burst"))
            .unwrap();
            assert_eq!(buf, msg.as_bytes(), "dial {i}: reply corrupted under burst");
            stream.send.finish().unwrap();
        }));
    }
    for t in tasks {
        t.await.expect("burst task panicked");
    }

    host.shutdown().await.unwrap();
}

/// The relay transport addr the host's endpoint currently advertises.
fn relay_url_of(node: &RoamingNode) -> Option<iroh::RelayUrl> {
    node.endpoint()
        .addr()
        .addrs
        .into_iter()
        .find_map(|a| match a {
            TransportAddr::Relay(url) => Some(url),
            _ => None,
        })
}

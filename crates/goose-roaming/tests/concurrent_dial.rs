use std::sync::Arc;

use futures::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use goose_roaming::{
    AcpStreamServer, Directory, RelayEntry, RelaySettings, RoamingConfig, RoamingIdentity,
    RoamingNode, TrustBook,
};
use iroh::{EndpointId, TransportAddr};

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

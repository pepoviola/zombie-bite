use std::{
    io::{self, Write},
    time::Duration,
};

use crate::utils::get_random_port;

use reqwest::Url;
use tracing::{debug, trace, warn};
use zombienet_orchestrator::metrics::{Metrics, MetricsHelper};
use zombienet_provider::{
    types::SpawnNodeOptions,
    DynNamespace, DynNode,
};
use zombienet_support::net::wait_ws_ready;

pub async fn sync_relay_only(
    ns: DynNamespace,
    cmd: impl AsRef<str>,
    chain: impl AsRef<str>,
    para_heads_env: Vec<(String, String)>,
) -> Result<(DynNode, String, String), ()> {
    println!("paras: \n {:?}", para_heads_env);
    let sync_db_path = if chain.as_ref() == "rococo" {
        "/tmp/snaps/rococo-snap/".to_string()
    } else {
        format!("{}/sync-db", ns.base_dir().to_string_lossy())
    };

    let env = if std::env::var("ZOMBIE_DUMP").is_ok() {
        [para_heads_env, vec![("ZOMBIE_DUMP".to_string(), "1".to_string())]].concat()
    } else {
        para_heads_env
    };

    let metrics_random_port = get_random_port().await;
    let opts = SpawnNodeOptions::new("sync-node", cmd.as_ref()).args(vec![
        "--chain",
        chain.as_ref(),
        "--sync",
        "warp",
        "-d",
        &sync_db_path,
        // "--rpc-port",
        // &rpc_random_port.to_string(),
        "--prometheus-port",
        &metrics_random_port.to_string(),
    ])
    .env(env);

    let sync_node = ns.spawn_node(&opts).await.unwrap();
    let metrics_url = format!("http://127.0.0.1:{metrics_random_port}/metrics");

    debug!("prometheus link http://127.0.0.1:{metrics_random_port}/metrics");
    println!("sync node logs: {}", sync_node.log_cmd());

    wait_ws_ready(&metrics_url).await.unwrap();
    let url = reqwest::Url::try_from(metrics_url.as_str()).unwrap();
    wait_sync(url).await.unwrap();
    println!("✅ Synced (chain: {})", chain.as_ref());
    // we should just paused
    Ok((sync_node, sync_db_path, chain.as_ref().to_string()))
}

pub async fn sync_para(
    ns: DynNamespace,
    cmd: impl AsRef<str>,
    chain: impl AsRef<str>,
    relaychain: impl AsRef<str>,
    relaychain_rpc_port: u16,
) -> Result<(DynNode, String, String), ()> {
    println!("pase!");
    let relay_rpc_url = format!("ws://localhost:{relaychain_rpc_port}");
    // wait_ws_ready(&relay_rpc_url).await.unwrap();
    let sync_db_path = format!(
        "{}/paras/{}/sync-db",
        ns.base_dir().to_string_lossy(),
        chain.as_ref()
    );
    let rpc_random_port = get_random_port().await;
    let metrics_random_port = get_random_port().await;
    let env = if std::env::var("ZOMBIE_DUMP").is_ok() {
        vec![("ZOMBIE_DUMP", "1")]
    } else {
        vec![]
    };

    let opts = SpawnNodeOptions::new("sync-node-para", cmd.as_ref()).args(vec![
        "--chain",
        chain.as_ref(),
        "--sync",
        "warp",
        "-d",
        &sync_db_path,
        "--rpc-port",
        &rpc_random_port.to_string(),
        "--prometheus-port",
        &metrics_random_port.to_string(),
        "--relay-chain-rpc-url",
        "wss://polkadot-rpc.dwellir.com",
        // &format!("ws://localhost:{relaychain_rpc_port}"),
        "--",
        "--chain",
        relaychain.as_ref(),
    ])
    .env(env);


    println!("{:?}", opts);
    let sync_node = ns.spawn_node(&opts).await.unwrap();
    let metrics_url = format!("http://127.0.0.1:{metrics_random_port}/metrics");

    debug!("prometheus link http://127.0.0.1:{metrics_random_port}/metrics");
    println!("sync para logs: {}", sync_node.log_cmd());

    wait_ws_ready(&metrics_url).await.unwrap();
    let url = reqwest::Url::try_from(metrics_url.as_str()).unwrap();
    wait_sync(url).await.unwrap();
    println!("✅ Synced (chain: {}), stopping node.", chain.as_ref());
    // we should just paused
    // sync_node.destroy().await.unwrap();
    Ok((sync_node, sync_db_path, chain.as_ref().to_string()))
}


// TODO: FIX terminal output on multiple tasks
async fn wait_sync(url: impl Into<Url>) -> Result<(), anyhow::Error> {
    const TERMINAL_WIDTH: u32 = 80;
    let url = url.into();

    print!("Syncing");
    let mut q = TERMINAL_WIDTH;
    // remove the first message
    q -= 7;
    // while let 1_f64 = Metrics::metric_with_url("substrate_sub_libp2p_is_major_syncing", url.clone())
    //     .await
    //     .unwrap()
    while is_syncing(url.clone()).await {
        if q == 0 {
            print!("\x1b[2K"); // Clear the whole line
            print!("\x1b[80D"); // Move to the start of the line
            print!("Syncing");
            io::stdout().flush().unwrap();
            q = TERMINAL_WIDTH - 7;
        }
        print!(".");
        io::stdout().flush().unwrap();
        q -= 1;
        tokio::time::sleep(Duration::from_secs(1)).await;
    }

    // ensure new line
    println!();

    Ok(())
}

async fn is_syncing(url: Url) -> bool {
    let metric = Metrics::metric_with_url("substrate_sub_libp2p_is_major_syncing", url).await;
    if let Ok(m) = metric {
        m == 1_f64
    } else {
        // Error getting metric
        false
    }
}

use std::fs::File;
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use flate2::Compression;
use tar::Builder;
use flate2::write::GzEncoder;

use zombienet_configuration::NetworkConfigBuilder;
use zombienet_orchestrator::network::Network;
use zombienet_orchestrator::Orchestrator;
use zombienet_provider::types::RunCommandOptions;
use zombienet_provider::types::SpawnNodeOptions;
use zombienet_provider::DynNamespace;
use zombienet_provider::DynProvider;
use zombienet_provider::Provider;
use zombienet_provider::NativeProvider;
use zombienet_support::fs::local::LocalFileSystem;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    // Star the node and wait until finish (with temp dir managed by us)
    println!("\n🪞 Starting DoppelGanger process for Kusama");

    let filesystem = LocalFileSystem;
    let provider = NativeProvider::new(filesystem.clone());
    let epoch_ms = get_epoch_ms();
    let fixed_base_dir = PathBuf::from_str(format!("/tmp/zombie-bite_{epoch_ms}").as_str()).unwrap();
    let base_dir_str = fixed_base_dir.to_string_lossy();
    let ns = provider
        .create_namespace_with_base_dir(fixed_base_dir.as_path())
        .await
        .unwrap();


    let _stdout = run_doppelganger_node(ns.clone(), &fixed_base_dir).await.unwrap();

    // get the chain-spec (prod) and clean the bootnodes
    let chain_spec_path = format!("{}/spec.json", &base_dir_str);
    generate_chain_spec(ns.clone(), &chain_spec_path).await.unwrap();

    // generate the data.tgz to use as snapshot
    let snap_path = format!("{}/snap.tgz", &base_dir_str);
    generate_snap(&base_dir_str, &snap_path).await.unwrap();

    // config a new network with alice/bob
	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain("kusama")
				.with_default_command("polkadot")
                .with_chain_spec_path(PathBuf::from(&chain_spec_path))
                .with_default_db_snapshot(PathBuf::from(&snap_path))
				.with_node(|node| node.with_name("alice"))
				.with_node(|node| node.with_name("bob"))
		})
        .build()
        .unwrap();

    // spawn the network
    let orchestrator = Orchestrator::new(filesystem, provider);
    let _network = orchestrator.spawn(config).await.unwrap();

    println!("🚀🚀🚀🚀 network deployed");

    // For now let just loop....
    #[allow(clippy::empty_loop)]
    loop {}
}

async fn spawn(provider: DynProvider, chain_spec_path: &str, snap_path: &str) -> Result<Network<LocalFileSystem>, String> {
    // config a new network with alice/bob
	let config = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain("kusama")
				.with_default_command("polkadot")
                .with_chain_spec_path(PathBuf::from(&chain_spec_path))
                .with_default_db_snapshot(PathBuf::from(&snap_path))
				.with_node(|node| node.with_name("alice"))
				.with_node(|node| node.with_name("bob"))
		})
        .build()
        .unwrap();

    // spawn the network
    let filesystem = LocalFileSystem;
    let orchestrator = Orchestrator::new(filesystem, provider);
    let network = orchestrator.spawn(config).await.unwrap();
    Ok(network)
}
async fn generate_snap(base_dir: &str, snap_path: &str) -> Result<(), String> {
    println!("\n📝 Generating snapshot");

    let compressed_file = File::create(&snap_path).unwrap();
    let mut encoder = GzEncoder::new(compressed_file, Compression::fast());
    let data_path = format!("{}/sync_db", &base_dir);

    let mut archive = Builder::new(&mut encoder);
    archive.append_dir_all("data", &data_path).unwrap();
    archive.finish().unwrap();

    println!("✅ generated with path {snap_path}");
    Ok(())

}

async fn generate_chain_spec(ns: DynNamespace, chain_spec_path: &str) -> Result<(), String> {
    println!("\n📝 Generating chain-spec without bootnodes...");


    let temp_node =
    ns.spawn_node(
        &SpawnNodeOptions::new("temp-polkadot", "bash")
            .args(vec![
                "-c", "while :; do sleep 60; done"
                ]),
    )
    .await.unwrap();

    let cmd_stdout = temp_node
        .run_command(RunCommandOptions::new("polkadot")
        .args(vec![
            "build-spec",
            "--chain",
            "kusama",
        ]))
        .await
        .unwrap()
        .unwrap();

    temp_node.destroy().await.unwrap();

    let mut chain_spec_json: serde_json::Value = serde_json::from_str(&cmd_stdout).unwrap();
    chain_spec_json["bootNodes"] = serde_json::Value::Array(vec![]);
    let contents = serde_json::to_string_pretty(&chain_spec_json).unwrap();

    tokio::fs::write(&chain_spec_path, contents).await.unwrap();
    println!("✅ generated with path {chain_spec_path}");

    Ok(())
}

async fn run_doppelganger_node(ns: DynNamespace, base_path: &Path) -> Result<(), String> {
    let data_path = format!("{}/sync_db", &base_path.to_string_lossy());
    let logs_path = format!("{}/sync.log", &base_path.to_string_lossy());
    println!("⛓  Syncing using warp, this could take a while. You can follow the logs with: \n\t
    tail -f {}", &logs_path);

    let temp_node =
    ns.spawn_node(
        &SpawnNodeOptions::new("temp-doppelganger", "bash")
            .args(vec![
                "-c", "while :; do sleep 60; done"
                ]),
    )
    .await.unwrap();

    let _stdout = temp_node
        .run_command(RunCommandOptions::new("bash")
        .args(vec![
            "-c", format!("doppelganger --chain kusama --sync warp -d {} > {} 2>&1", &data_path, &logs_path).as_str(),
        ]))
        .await
        .unwrap();

    temp_node.destroy().await.unwrap();

     println!("✅ Synced");

    Ok(())
}

fn get_epoch_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis()
}


#[cfg(test)]
mod test {
    use super::*;

    #[tokio::test(flavor = "multi_thread")]
    async fn test_snap() {
        let snap_path = "/tmp/zombie-bite_1726677980197/snap.tgz";
        let demo = generate_snap("/tmp/zombie-bite_1726677980197", &snap_path).await.unwrap();
        println!("{:?}", demo);
        // let _n = spawn(provider, chain_spec_path, snap_path).await.unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_spawn() {
        let filesystem = LocalFileSystem;
        let provider = NativeProvider::new(filesystem.clone());
        let chain_spec_path = "/tmp/zombie-bite_1726677980197/spec.json";
        let snap_path = "/tmp/zombie-bite_1726677980197/snap.tgz";
        let _n = spawn(provider, chain_spec_path, snap_path).await.unwrap();
    }

}
use std::{
    env,
    io::{self, Write},
    path::PathBuf,
    str::FromStr,
    time::Duration,
};

use reqwest::Url;
use subalfred_core::state::fork_off::{fork_off, ForkOffConfig};
use tokio::net::TcpListener;
use tracing::{debug, trace};
use zombienet_configuration::RegistrationStrategy;
use zombienet_configuration::{types::AssetLocation, NetworkConfig};
use zombienet_orchestrator::Orchestrator;
use zombienet_orchestrator::{
    metrics::{Metrics, MetricsHelper},
    network::Network,
    NetworkSpec, ScopedFilesystem,
};
use zombienet_provider::{
    types::{RunCommandOptions, SpawnNodeOptions},
    DynNamespace, DynNode, DynProvider, NativeProvider, Provider,
};
use zombienet_support::{fs::local::LocalFileSystem, net::wait_ws_ready};

async fn sync(ns: DynNamespace) -> Result<(DynNode, String), ()> {
    let sync_db_path = format!("{}/sync-db", ns.base_dir().to_string_lossy());
    let rpc_random_port = get_random_port().await;
    let metrics_random_port = get_random_port().await;
    let opts = SpawnNodeOptions::new("sync-node", "polkadot").args(vec![
        "--chain",
        "polkadot",
        "--sync",
        "warp",
        "-d",
        &sync_db_path,
        "--rpc-port",
        &rpc_random_port.to_string(),
        "--prometheus-port",
        &metrics_random_port.to_string(),
    ]);

    let sync_node = ns.spawn_node(&opts).await.unwrap();
    let metrics_url = format!("http://127.0.0.1:{metrics_random_port}/metrics");

    println!("prometheus link http://127.0.0.1:{metrics_random_port}/metrics");
    println!("logs: {}", sync_node.log_cmd());

    wait_ws_ready(&metrics_url).await.unwrap();
    let url = reqwest::Url::try_from(metrics_url.as_str()).unwrap();
    wait_sync(url).await.unwrap();
    println!("sync ok!, stopping node");
    // we should just paused
    sync_node.destroy().await.unwrap();
    Ok((sync_node, sync_db_path))
}

async fn export_state(sync_node: DynNode, sync_db_path: String) -> Result<String, ()> {
    let exported_state_file = format!("{sync_db_path}/exported-state.json");
    let cmd_opts =
        RunCommandOptions::new("polkadot").args(vec!["export-state", "-d", &sync_db_path]);
    debug!("cmd: {:?}", cmd_opts);
    let exported_state_content = sync_node.run_command(cmd_opts).await.unwrap().unwrap();
    tokio::fs::write(&exported_state_file, exported_state_content)
        .await
        .unwrap();

    println!("State exported to {exported_state_file}");
    Ok(exported_state_file)
}

async fn generate_new_network(
    network_def: impl AsRef<str>,
    ns: DynNamespace,
    base_dir: impl AsRef<str>,
) -> Result<NetworkSpec, anyhow::Error> {
    let filesystem = LocalFileSystem;
    let config = NetworkConfig::load_from_toml(network_def.as_ref()).unwrap();
    let mut spec = zombienet_orchestrator::NetworkSpec::from_config(&config)
        .await
        .unwrap();
    let scoped_fs = ScopedFilesystem::new(&filesystem, base_dir.as_ref());

    {
        let relaychain = spec.relaychain_mut();
        let chain_spec = relaychain.chain_spec_mut();
        trace!("{chain_spec:?}");
        chain_spec.build(&ns, &scoped_fs).await.unwrap();
        trace!("{chain_spec:?}");
        let relaychain_id = chain_spec.read_chain_id(&scoped_fs).await.unwrap();

        spec.build_parachain_artifacts(ns.clone(), &scoped_fs, &relaychain_id, true)
            .await
            .unwrap();
    }

    override_paras_asset_location(&mut spec, base_dir.as_ref());

    let mut para_artifacts = vec![];
    {
        let paras_in_genesis = spec
            .parachains_iter()
            .filter(|para| para.registration_strategy() == &RegistrationStrategy::InGenesis)
            .collect::<Vec<_>>();

        for para in paras_in_genesis {
            {
                let genesis_config = para.get_genesis_config().unwrap();
                para_artifacts.push(genesis_config)
            }
        }
    }

    {
        let relaychain = spec.relaychain();
        let chain_spec = relaychain.chain_spec();
        // Customize relaychain
        chain_spec
            .customize_relay::<_, &PathBuf>(relaychain, &[], para_artifacts, &scoped_fs)
            .await
            .unwrap();
    }

    // create raw version of chain-spec
    let relaychain = spec.relaychain_mut();
    let chain_spec = relaychain.chain_spec_mut();
    chain_spec.build_raw(&ns, &scoped_fs).await.unwrap();

    Ok(spec)
}

async fn bite(
    fixed_base_dir: impl AsRef<str>,
    chain_spec_raw_path: impl AsRef<str>,
    exported_state_file: impl AsRef<str>,
) -> Result<PathBuf, anyhow::Error> {
    let fork_off_config = ForkOffConfig {
        renew_consensus_with: Some(format!(
            "{}/{}",
            fixed_base_dir.as_ref(),
            chain_spec_raw_path.as_ref()
        )),
        simple_governance: false,
        disable_default_bootnodes: true,
    };
    trace!("{:?}", fork_off_config);
    println!("{}", exported_state_file.as_ref());

    let r = fork_off(exported_state_file.as_ref(), &fork_off_config);
    trace!("{:?}", r);
    let forked_off_path =
        PathBuf::from_str(&format!("{}.fork-off", exported_state_file.as_ref())).unwrap();
    Ok(forked_off_path)
}

async fn spawn_forked_network(
    provider: DynProvider,
    mut spec: NetworkSpec,
    forked_off_path: impl Into<PathBuf>,
) -> Result<Network<LocalFileSystem>, anyhow::Error> {
    // let forked_off_path  = PathBuf::from_str(&format!("{exported_state_file}.fork-off")).unwrap();
    let relaychain = spec.relaychain_mut();
    let chain_spec = relaychain.chain_spec_mut();
    chain_spec.set_asset_location(AssetLocation::FilePath(forked_off_path.into()));

    trace!("{:?}", spec);

    let filesystem = LocalFileSystem;
    let orchestrator = Orchestrator::new(filesystem, provider);
    let network = orchestrator.spawn_from_spec(spec).await.unwrap();
    Ok(network)
}

async fn wait_sync(url: impl Into<Url>) -> Result<(), anyhow::Error> {
    const TERMINAL_WIDTH: u32 = 80;
    let url = url.into();

    print!("Syncing");
    let mut q = TERMINAL_WIDTH;
    // remove the first message
    q -= 7;
    while let 1_f64 = Metrics::metric_with_url("substrate_sub_libp2p_is_major_syncing", url.clone())
        .await
        .unwrap()
    {
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

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let args: Vec<_> = env::args().collect();
    println!("{:?}", args);

    if args.len() < 2 {
        panic!(
            "Missing argument (path to network definition):
        \t zombie-bite <path to network definition>
        "
        );
    }

    // DEMO "/tmp/demo.toml"
    let path_to_network_def = args[1].clone();

    let filesystem = LocalFileSystem;
    let provider = NativeProvider::new(filesystem.clone());
    let fixed_base_dir = PathBuf::from_str("/tmp/z").unwrap();
    let base_dir_str = fixed_base_dir.to_string_lossy();
    let ns = provider
        .create_namespace_with_base_dir(fixed_base_dir.as_path())
        .await
        .unwrap();

    let (sync_node, sync_db_path) = sync(ns.clone()).await.unwrap();
    let exported_state_filepath = export_state(sync_node, sync_db_path).await.unwrap();
    let spec = generate_new_network(&path_to_network_def, ns.clone(), &base_dir_str)
        .await
        .unwrap();
    let chain_spec_raw_generated = spec
        .relaychain()
        .chain_spec()
        .raw_path()
        .unwrap()
        .to_string_lossy();
    let forked_filepath = bite(
        &base_dir_str,
        chain_spec_raw_generated,
        &exported_state_filepath,
    )
    .await
    .unwrap();
    let _network = spawn_forked_network(provider.clone(), spec, forked_filepath)
        .await
        .unwrap();

    println!("looping...");

    loop {
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}

fn override_paras_asset_location(spec: &mut NetworkSpec, base_dir: &str) {
    for para in spec.parachains_iter_mut() {
        if let Some(chain_spec) = para.chain_spec_mut() {
            // raw path should be already created
            if let Some(raw_path) = chain_spec.raw_path() {
                let full_path = PathBuf::from_iter([base_dir, &raw_path.to_string_lossy()]);
                chain_spec.set_asset_location(AssetLocation::FilePath(full_path));
            }
        }
    }
}

async fn get_random_port() -> u16 {
    let listener = TcpListener::bind("0.0.0.0:0".to_string())
        .await
        .expect("Can't bind a random port");

    listener
        .local_addr()
        .expect("We should always get the local_addr from the listener, qed")
        .port()
}

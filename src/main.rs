use std::{
    env,
    io::{self, Write},
    path::PathBuf,
    str::FromStr,
    time::Duration,
};

use config::Context;
use futures::future::try_join_all;
use futures::FutureExt;
use reqwest::Url;
use tracing::{debug, info, trace, warn};
use zombienet_configuration::types::AssetLocation;
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
use zombienet_support::net::wait_ws_ready;
use zombienet_sdk::LocalFileSystem;

mod utils;
use utils::get_random_port;
mod chain_spec_raw;
mod config;
mod sync;
mod cli;

mod fork_off;
use fork_off::{fork_off, ForkOffConfig};

use crate::fork_off::ParasHeads;

async fn sync_relay_only(
    ns: DynNamespace,
    chain: impl AsRef<str>,
    rpc_random_port: u16,
) -> Result<(DynNode, String), ()> {
    let sync_db_path = if chain.as_ref() == "rococo" {
        "/tmp/snaps/rococo-snap/".to_string()
    } else {
        format!("{}/sync-db", ns.base_dir().to_string_lossy())
    };

    let metrics_random_port = get_random_port().await;
    let opts = SpawnNodeOptions::new("sync-node", "polkadot").args(vec![
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
    ]);

    let sync_node = ns.spawn_node(&opts).await.unwrap();
    let metrics_url = format!("http://127.0.0.1:{metrics_random_port}/metrics");

    debug!("prometheus link http://127.0.0.1:{metrics_random_port}/metrics");
    info!("sync node logs: {}", sync_node.log_cmd());

    wait_ws_ready(&metrics_url).await.unwrap();
    let url = reqwest::Url::try_from(metrics_url.as_str()).unwrap();
    wait_sync(url).await.unwrap();
    info!("sync ok!, stopping node");
    // we should just paused
    // sync_node.destroy().await.unwrap();
    Ok((sync_node, sync_db_path))
}

async fn sync_para(
    ns: DynNamespace,
    chain: impl AsRef<str>,
    relaychain: impl AsRef<str>,
    relaychain_rpc_port: u16,
) -> Result<(DynNode, String), ()> {
    let relay_rpc_url = format!("ws://localhost:{relaychain_rpc_port}");
    wait_ws_ready(&relay_rpc_url).await.unwrap();
    let sync_db_path = format!(
        "{}/paras/{}/sync-db",
        ns.base_dir().to_string_lossy(),
        chain.as_ref()
    );
    let rpc_random_port = get_random_port().await;
    let metrics_random_port = get_random_port().await;
    let opts = SpawnNodeOptions::new("sync-node-para", "polkadot-parachain").args(vec![
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
        &format!("ws://localhost:{relaychain_rpc_port}"),
        "--",
        "--chain",
        relaychain.as_ref(),
    ]);

    debug!("{:?}", opts);
    let sync_node = ns.spawn_node(&opts).await.unwrap();
    let metrics_url = format!("http://127.0.0.1:{metrics_random_port}/metrics");

    debug!("prometheus link http://127.0.0.1:{metrics_random_port}/metrics");
    info!("sync para logs: {}", sync_node.log_cmd());

    wait_ws_ready(&metrics_url).await.unwrap();
    let url = reqwest::Url::try_from(metrics_url.as_str()).unwrap();
    wait_sync(url).await.unwrap();
    println!("sync ok!, stopping node");
    // we should just paused
    sync_node.destroy().await.unwrap();
    Ok((sync_node, sync_db_path))
}

async fn export_state(
    sync_node: DynNode,
    sync_db_path: String,
    chain: &str,
    context: Context,
) -> Result<String, ()> {
    let exported_state_file = format!("{sync_db_path}/exported-state.json");

    let cmd_opts = RunCommandOptions::new(context.cmd()).args(vec![
        "export-state",
        "--chain",
        chain,
        "-d",
        &sync_db_path,
    ]);
    debug!("cmd: {:?}", cmd_opts);
    let exported_state_content = sync_node.run_command(cmd_opts).await.unwrap().unwrap();
    tokio::fs::write(&exported_state_file, exported_state_content)
        .await
        .unwrap();

    info!("State exported to {exported_state_file}");
    Ok(exported_state_file)
}

async fn generate_new_network(
    relay: &config::Relaychain,
    ns: DynNamespace,
    base_dir: impl AsRef<str>,
    paras: Vec<config::Parachain>,
) -> Result<NetworkSpec, anyhow::Error> {
    let filesystem = LocalFileSystem;
    let config = config::generate_network_config(relay, paras).unwrap();
    let mut spec = zombienet_orchestrator::NetworkSpec::from_config(&config)
        .await
        .unwrap();
    let scoped_fs = ScopedFilesystem::new(&filesystem, base_dir.as_ref());

    let relaychain_id = {
        let relaychain = spec.relaychain_mut();
        let chain_spec = relaychain.chain_spec_mut();
        trace!("{chain_spec:?}");
        chain_spec.build(&ns, &scoped_fs).await.unwrap();
        trace!("{chain_spec:?}");
        let relaychain_id = chain_spec.read_chain_id(&scoped_fs).await.unwrap();

        spec.build_parachain_artifacts(ns.clone(), &scoped_fs, &relaychain_id, true)
            .await
            .unwrap();

        relaychain_id
    };

    override_paras_asset_location(&mut spec, base_dir.as_ref());

    let mut para_artifacts = vec![];
    // TODO: do we need to add custom paras?
    // {
    //     let paras_in_genesis = spec
    //         .parachains_iter()
    //         .filter(|para| para.registration_strategy() == &RegistrationStrategy::InGenesis)
    //         .collect::<Vec<_>>();

    //     for para in paras_in_genesis {
    //         {
    //             let genesis_config = para.get_genesis_config().unwrap();
    //             para_artifacts.push(genesis_config)
    //         }
    //     }
    // }
    {
        for para in spec.parachains_iter() {
            {
                let genesis_config = para.get_genesis_config().unwrap();
                para_artifacts.push(genesis_config)
            }
        }
    }

    // customize chain-spec for paras
    {
        for para in spec.parachains_iter() {
            let para_chain_spec = para.chain_spec().unwrap();
            para_chain_spec
                .customize_para(para, &relaychain_id, &scoped_fs)
                .await
                .unwrap();
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

    {
        for para in spec.parachains_iter_mut() {
            let para_chain_spec = para.chain_spec_mut().unwrap();
            para_chain_spec.build_raw(&ns, &scoped_fs).await.unwrap();
        }
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
    context: config::Context,
    paras_head: ParasHeads,
) -> Result<PathBuf, anyhow::Error> {
    let fork_off_config = ForkOffConfig {
        renew_consensus_with: format!(
            "{}/{}",
            fixed_base_dir.as_ref(),
            chain_spec_raw_path.as_ref()
        ),
        simple_governance: false,
        disable_default_bootnodes: true,
        paras_heads: paras_head,
    };
    trace!("{:?}", fork_off_config);
    info!("{}", exported_state_file.as_ref());
    info!("{:?}", fork_off_config);

    let forked_off_path = fork_off(exported_state_file.as_ref(), &fork_off_config, context).await?;
    info!("{:?}", forked_off_path);

    Ok(forked_off_path)
}

async fn spawn_forked_network(
    provider: DynProvider,
    mut spec: NetworkSpec,
    forked_off_path: impl Into<PathBuf>,
    paras_forked_off_paths: Vec<PathBuf>,
) -> Result<Network<LocalFileSystem>, anyhow::Error> {
    {
        let relaychain = spec.relaychain_mut();
        let chain_spec = relaychain.chain_spec_mut();
        chain_spec.set_asset_location(AssetLocation::FilePath(forked_off_path.into()));
    }

    {
        let mut paras = spec.parachains_iter_mut();
        let mut paras_forked = paras_forked_off_paths.into_iter();

        for para in paras.by_ref() {
            let chain_asset_location = paras_forked.next().unwrap();
            let chain_spec = para.chain_spec_mut().unwrap();
            chain_spec.set_asset_location(AssetLocation::FilePath(chain_asset_location));
        }
    }

    debug!("{:?}", spec);

    let filesystem = LocalFileSystem;
    let orchestrator = Orchestrator::new(filesystem, provider);
    let network = orchestrator.spawn_from_spec(spec).await.unwrap();
    Ok(network)
}

// TODO: FIX terminal output on multiple tasks
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

    if args.len() < 2 {
        panic!(
            "Missing argument (network to bite...):
        \t zombie-bite <polkadot|kusama> [asset-hub, coretime, people]
        "
        );
    }

    let (relay_chain, paras_to) = cli::parse(args);

    info!("Syncing {} and paras: {:?}",relay_chain.as_chain_string(), paras_to);
    let filesystem = LocalFileSystem;
    let provider = NativeProvider::new(filesystem.clone());
    let fixed_base_dir = PathBuf::from_str("/tmp/z").unwrap();
    let base_dir_str = fixed_base_dir.to_string_lossy();
    let ns = provider
        .create_namespace_with_base_dir(fixed_base_dir.as_path())
        .await
        .unwrap();

    let relaychain_rpc_random_port = get_random_port().await;
    let mut syncs = vec![sync_relay_only(
        ns.clone(),
        relay_chain.as_chain_string(),
        relaychain_rpc_random_port,
    )
    .boxed()];
    for para in &paras_to {
        syncs.push(
            sync_para(
                ns.clone(),
                para.as_chain_string(&relay_chain.as_chain_string()),
                relay_chain.as_chain_string(),
                relaychain_rpc_random_port,
            )
            .boxed(),
        );
    }

    // syncs.push(sync_relay_only(ns.clone(), relay_chain.as_chain_string(), relaychain_rpc_random_port));
    let res = try_join_all(syncs).await.unwrap();
    let mut res_iter = res.into_iter();
    let (sync_node, sync_db_path) = res_iter.next().unwrap();
    // stop relay node
    sync_node.destroy().await.unwrap();

    // We need to build first the parachains artifacts to include the new `Head` in the relaychain
    let mut paras_exported_state_paths = vec![];
    for para in &paras_to {
        // export para state
        let (para_sync_node, para_sync_db_path) = res_iter.next().unwrap();
        let para_exported_state_filepath = export_state(
            para_sync_node,
            para_sync_db_path,
            &para.as_chain_string(&relay_chain.as_chain_string()),
            Context::Parachain,
        )
        .await
        .unwrap();
        paras_exported_state_paths.push(para_exported_state_filepath);
    }

    let exported_state_filepath = export_state(
        sync_node,
        sync_db_path,
        &relay_chain.as_chain_string(),
        Context::Relaychain,
    )
    .await
    .unwrap();

    let mut spec = generate_new_network(&relay_chain, ns.clone(), &base_dir_str, paras_to.clone())
        .await
        .unwrap();

    // bite paras
    let mut paras_iter = spec.parachains_iter_mut();
    let mut paras_exported_state_iter = paras_exported_state_paths.iter();
    let mut paras_forked_off_paths: Vec<PathBuf> = vec![];
    let mut paras_heads: ParasHeads = Default::default();

    for para in &paras_to {
        let para_spec = paras_iter.next().unwrap();
        let para_raw_generated_path = para_spec
            .chain_spec()
            .unwrap()
            .raw_path()
            .unwrap()
            .to_string_lossy();
        // bite para
        let para_forked_path = bite(
            &base_dir_str,
            para_raw_generated_path,
            paras_exported_state_iter.next().unwrap(),
            para.context(),
            Default::default(),
        )
        .await
        .unwrap();

        paras_forked_off_paths.push(para_forked_path.clone());

        // Update chainspec to get the new heads
        if let Some(chain_spec) = para_spec.chain_spec_mut() {
            chain_spec.set_asset_location(AssetLocation::FilePath(para_forked_path));
        }
    }

    drop(paras_iter);

    // rebuild all the para artifacts
    let scoped_fs = ScopedFilesystem::new(&filesystem, base_dir_str.as_ref());
    let relaychain_id = {
        let relaychain_spec = spec.relaychain().chain_spec();
        relaychain_spec.read_chain_id(&scoped_fs).await.unwrap()
    };
    spec.build_parachain_artifacts(ns.clone(), &scoped_fs, &relaychain_id, true)
        .await
        .unwrap();

    for para in spec.parachains_iter() {
        let id = para.id();
        let head_path = format!("{base_dir_str}/{id}/genesis-state");
        let head = tokio::fs::read_to_string(head_path).await.unwrap();
        paras_heads.insert(id, head);
    }

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
        relay_chain.context(),
        paras_heads,
    )
    .await
    .unwrap();

    let _network = spawn_forked_network(
        provider.clone(),
        spec,
        forked_filepath,
        paras_forked_off_paths,
    )
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

#![allow(dead_code)]
// TODO: don't allow dead_code

use futures::future::try_join_all;
use futures::{FutureExt, StreamExt};
use serde_json::json;
use std::fs::{read_to_string, File};
use std::path::Path;
use std::path::PathBuf;
use std::str::FromStr;
use std::time::UNIX_EPOCH;
use std::time::{Duration, SystemTime};
use tokio::fs;
use zombienet_sdk::NetworkNode;

use codec::Encode;
use flate2::write::GzEncoder;
use flate2::Compression;
use tar::Builder;

use tracing::info;
use tracing::{debug, trace, warn};
use zombienet_configuration::NetworkConfigBuilder;
use zombienet_orchestrator::network::Network;
use zombienet_orchestrator::Orchestrator;
use zombienet_provider::types::RunCommandOptions;
use zombienet_provider::types::SpawnNodeOptions;
use zombienet_provider::DynNamespace;
use zombienet_provider::DynProvider;
use zombienet_provider::NativeProvider;
use zombienet_provider::Provider;
use zombienet_support::fs::local::LocalFileSystem;

use crate::utils::{para_head_key, HeadData};

use crate::config::{Context, Parachain, Relaychain};
use crate::overrides::{generate_default_overrides_for_para, generate_default_overrides_for_rc};
use crate::sync::{sync_para, sync_relay_only};
use crate::utils::get_random_port;

use std::env;

const CHECK_TIMEOUT_SECS: u64 = 900; // 15 mins
const PORTS_FILE: &str = "ports.json";
const READY_FILE: &str = "ready.json";

#[derive(Debug, Clone)]
struct ChainArtifact {
    cmd: String,
    chain: String,
    spec_path: String,
    snap_path: String,
    override_wasm: Option<String>,
}

pub async fn doppelganger_inner(relay_chain: Relaychain, paras_to: Vec<Parachain>) {
    // Star the node and wait until finish (with temp dir managed by us)
    info!(
        "🪞 Starting DoppelGanger process for {} and {:?}",
        relay_chain.as_chain_string(),
        paras_to
    );

    let filesystem = LocalFileSystem;
    let provider = NativeProvider::new(filesystem.clone());
    let epoch_ms = get_epoch_ms();

    let global_base_dir = if let Ok(fixed_path) = env::var("ZOMBIE_BITE_BASE_PATH") {
        PathBuf::from_str(&fixed_path).unwrap()
    } else {
        PathBuf::from_str(format!("/tmp/zombie-bite_{epoch_ms}").as_str()).unwrap()
    };

    // ensure the base path exist
    tokio::fs::create_dir_all(&global_base_dir).await.unwrap();

    // add `/bite` to global base
    let fixed_base_dir = global_base_dir.canonicalize().unwrap().join("bite");

    let base_dir_str = fixed_base_dir.to_string_lossy();
    let ns = provider
        .create_namespace_with_base_dir(fixed_base_dir.as_path())
        .await
        .unwrap();

    let _relaychain_rpc_random_port = get_random_port().await;

    // Parachain sync
    let mut syncs = vec![];
    for para in &paras_to {
        let para_default_overrides_path =
            generate_default_overrides_for_para(&base_dir_str, para, &relay_chain).await;
        let info_path = format!("{base_dir_str}/para-{}.txt", para.id());

        syncs.push(
            sync_para(
                ns.clone(),
                "doppelganger-parachain",
                para.as_chain_string(&relay_chain.as_chain_string()),
                relay_chain.as_chain_string(),
                relay_chain.sync_endpoint(),
                para_default_overrides_path,
                info_path,
            )
            .boxed(),
        );
    }

    let res = try_join_all(syncs).await.unwrap();

    // loop over paras
    let mut para_artifacts = vec![];
    let mut para_heads_env = vec![];
    let context_para = Context::Parachain;
    for (para_index, (_sync_node, sync_db_path, sync_chain, sync_head_path)) in
        res.into_iter().enumerate()
    {
        let sync_chain_name = if sync_chain.contains('/') {
            let parts: Vec<&str> = sync_chain.split('/').collect();
            parts.last().unwrap().to_string()
        } else {
            // is not a file
            sync_chain.clone()
        };

        let chain_spec_path = format!("{}/{}-spec.json", &base_dir_str, &sync_chain_name);
        generate_chain_spec(
            ns.clone(),
            &chain_spec_path,
            &context_para.cmd(),
            &sync_chain,
        )
        .await
        .unwrap();

        // generate the data.tgz to use as snapshot
        let snap_path = format!("{}/{}-snap.tgz", &base_dir_str, &sync_chain_name);
        println!("s: {snap_path}");
        generate_snap(&sync_db_path, &snap_path).await.unwrap();

        // // real last log line to get the para_head
        // let logs = sync_node
        //     .logs()
        //     .await
        //     .expect("read logs from node should work");
        // let para_head_str = logs
        //     .lines()
        //     .last()
        //     .expect("last line should be valid.")
        //     .to_string();

        let para_head_str = read_to_string(&sync_head_path).expect(&format!(
            "read para_head ({sync_head_path}) file should works."
        ));
        let para_head_hex = if &para_head_str[..2] == "0x" {
            &para_head_str[2..]
        } else {
            &para_head_str
        };

        let para_head = array_bytes::bytes2hex(
            "0x",
            HeadData(hex::decode(para_head_hex).expect("para_head should be a valid hex. qed"))
                .encode(),
        );

        let para = paras_to
            .get(para_index)
            .expect("para_index should be valid. qed");
        para_heads_env.push((
            format!("ZOMBIE_{}", &para_head_key(para.id())[2..]),
            format!("{}", &para_head[2..]),
        ));

        para_artifacts.push(ChainArtifact {
            cmd:  context_para.cmd(),
            chain: if sync_chain.contains('/') { para.as_chain_string(&relay_chain.as_chain_string()) } else { sync_chain },
            spec_path: chain_spec_path,
            snap_path,
            override_wasm: para.wasm_overrides().map(str::to_string),
        });
    }

    let rc_default_overrides_path =
        generate_default_overrides_for_rc(&base_dir_str, &relay_chain, &paras_to).await;
    let rc_info_path = format!("{base_dir_str}/rc_info.txt");
    // RELAYCHAIN sync
    let (sync_node, sync_db_path, sync_chain) = sync_relay_only(
        ns.clone(),
        "doppelganger",
        relay_chain.as_chain_string(),
        para_heads_env,
        rc_default_overrides_path,
        &rc_info_path,
    )
    .await
    .unwrap();

    // stop relay node
    sync_node.destroy().await.unwrap();

    // get the chain-spec (prod) and clean the bootnodes
    // relaychain
    let context_relay = Context::Relaychain;
    let r_chain_spec_path = format!("{}/{}-spec.json", &base_dir_str, &sync_chain);
    generate_chain_spec(
        ns.clone(),
        &r_chain_spec_path,
        &context_relay.cmd(),
        &sync_chain,
    )
    .await
    .unwrap();

    // remove `parachains` db
    let parachains_path = format!("{sync_db_path}/chains/{sync_chain}/db/full/parachains");
    debug!("Deleting `parachains` db at {parachains_path}");
    tokio::fs::remove_dir_all(parachains_path)
        .await
        .expect("remove parachains db should work");

    // generate the data.tgz to use as snapshot
    let r_snap_path = format!("{}/{}-snap.tgz", &base_dir_str, &sync_chain);
    generate_snap(&sync_db_path, &r_snap_path).await.unwrap();

    let relay_artifacts = ChainArtifact {
        cmd: "doppelganger".into(),
        chain: sync_chain,
        spec_path: r_chain_spec_path,
        snap_path: r_snap_path,
        override_wasm: relay_chain.wasm_overrides().map(str::to_string),
    };

    let network = spawn(
        provider,
        relay_artifacts,
        para_artifacts,
        Some(global_base_dir.clone()),
    )
    .await
    .expect("Fail to spawn the new network");

    info!("🚀🚀🚀🚀 network deployed");

    // collect info
    let alice = network.get_node("alice").unwrap();
    let bob = network.get_node("bob").unwrap();
    let collator = network.get_node("collator").unwrap();

    let rc_start_block = fs::read_to_string(format!("{base_dir_str}/rc_info.txt"))
        .await
        .unwrap()
        .parse::<u64>()
        .expect("read bite rc block should works");
    let ah_start_block = fs::read_to_string(format!("{base_dir_str}/para-1000.txt"))
        .await
        .unwrap()
        .parse::<u64>()
        .expect("read bite ah block should works");

    // ready to start
    let ready_content = json!({
        "rc_start_block": rc_start_block,
        "ah_start_block": ah_start_block,
    });

    // ports
    let ports_content = json!({
        "alice_port" : alice.ws_uri().split(":").nth(2).unwrap(),
        "collator_port": collator.ws_uri().split(":").nth(2).unwrap(),
    });

    // ensure block production
    let client = network
        .get_node("alice")
        .unwrap()
        .wait_client::<zombienet_sdk::subxt::PolkadotConfig>()
        .await
        .unwrap();
    let mut blocks = client.blocks().subscribe_finalized().await.unwrap().take(3);

    while let Some(block) = blocks.next().await {
        println!("Block #{}", block.unwrap().header().number);
    }

    if let Ok(ci_path) = env::var("ZOMBIE_BITE_CI_PATH") {
        // move artifacts to make it reusable

        // copy rc info to this CI directory
        let _ = fs::copy(rc_info_path, format!("{ci_path}/rc-info.txt")).await;
        // create info files
        let _ = fs::write(format!("{ci_path}/{PORTS_FILE}"), ports_content.to_string()).await;
        let _ = fs::write(format!("{ci_path}/{READY_FILE}"), ready_content.to_string()).await;

        info!("teardown network...");
        // shutdown the network
        // network.destroy().await.unwrap();
    } else {
        // monitoring block production every 15 mins
        // but first wait until the collator reply the metrics
        let _ = collator
            .wait_metric_with_timeout("node_roles", |x| x > 1.0, 300_u64)
            .await
            .unwrap();

        // create info files
        let _ = fs::write(
            format!("{}/{PORTS_FILE}", &global_base_dir.to_string_lossy()),
            ports_content.to_string(),
        )
        .await;
        let _ = fs::write(
            format!("{}/{READY_FILE}", &global_base_dir.to_string_lossy()),
            ready_content.to_string(),
        )
        .await;

        let mut alice_block = progress(alice, 0).await.expect("first check should works");
        let mut bob_block = progress(bob, 0).await.expect("first check should works");
        let mut collator_block = progress(collator, 0)
            .await
            .expect("first check should works");

        loop {
            tokio::time::sleep(Duration::from_secs(CHECK_TIMEOUT_SECS)).await;
            // check the progress
            // alice
            let mut alice_was_restarted = false;
            if let Ok(block) = progress(alice, alice_block).await {
                alice_block = block;
            } else {
                // restart alice / collator
                restart(alice, alice_block).await;
                restart(collator, collator_block).await;
                alice_was_restarted = true;
            }

            // bob
            if let Ok(block) = progress(bob, bob_block).await {
                bob_block = block;
            } else {
                // restart alice / collator
                restart(bob, bob_block).await;
            }

            if !alice_was_restarted {
                if let Ok(block) = progress(collator, collator_block).await {
                    collator_block = block;
                } else {
                    // restart alice / collator
                    restart(collator, collator_block).await;
                }
            }
        }
    }
}

async fn restart(node: &NetworkNode, checkpoint: impl Into<f64>) {
    if (node.restart(None).await).is_ok() {
        warn!(
            "{} was restarted at block {}",
            node.name(),
            checkpoint.into()
        );
    } else {
        warn!("Error restarting {}", node.name());
    }
}

async fn progress(node: &NetworkNode, checkpoint: impl Into<f64>) -> Result<f64, anyhow::Error> {
    let metric = node.reports("block_height{status=\"best\"}").await?;
    let checkpoint = checkpoint.into();
    if metric > checkpoint {
        trace!(
            "{} is making progress, checkpoint {} - current {}",
            node.name(),
            checkpoint,
            metric
        );

        Ok(metric)
    } else {
        Err(anyhow::anyhow!(
            "node don't progress, current {metric} - checkpoint {checkpoint}"
        ))
    }
}

async fn spawn(
    provider: DynProvider,
    relaychain: ChainArtifact,
    paras: Vec<ChainArtifact>,
    global_base_dir: Option<PathBuf>,
) -> Result<Network<LocalFileSystem>, String> {
    let leaked_rust_log = env::var("RUST_LOG").unwrap_or_else(|_| {
        String::from(
            "babe=trace,grandpa=info,runtime=trace,consensus::common=trace,parachain=debug",
        )
    });

    let (chain_spec_path, db_path) = if let Ok(ci_path) = env::var("ZOMBIE_BITE_CI_PATH") {
        let chain_spec_path = PathBuf::from(relaychain.spec_path.as_str());
        let chain_spec_filename = chain_spec_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();

        let db_path = PathBuf::from(relaychain.snap_path.as_str());
        let db_path_filename = db_path.file_name().unwrap().to_string_lossy().to_string();

        let new_chain_spec_path = PathBuf::from(&format!("{ci_path}/{}", chain_spec_filename));
        let new_db_path = PathBuf::from(&format!("{ci_path}/{}", db_path_filename));

        tokio::fs::rename(chain_spec_path, &new_chain_spec_path)
            .await
            .unwrap();
        tokio::fs::rename(db_path, &new_db_path).await.unwrap();

        (
            PathBuf::from(format!("./{}", chain_spec_filename)),
            PathBuf::from(format!("./{}", db_path_filename)),
        )
    } else {
        (
            PathBuf::from(relaychain.spec_path.as_str()),
            PathBuf::from(relaychain.snap_path.as_str()),
        )
    };

    let rpc_port: u16 = if let Ok(port) = env::var("ZOMBIE_BITE_RC_PORT") {
        port.parse()
            .expect("env var ZOMBIE_BITE_RC_PORT must be a valid u16")
    } else {
        get_random_port().await
    };

    // config a new network with alice/bob
    let mut config = NetworkConfigBuilder::new().with_relaychain(|r| {
        let relay_builder = r
            .with_chain(relaychain.chain.as_str())
            .with_default_command(relaychain.cmd.as_str())
            .with_chain_spec_path(chain_spec_path)
            .with_default_db_snapshot(db_path)
            .with_default_args(vec![
                ("-l", leaked_rust_log.as_str()).into(),
                "--discover-local".into(),
                "--allow-private-ip".into(),
                "--no-hardware-benchmarks".into(),
            ]);

        // We override the code directly in the db
        // relay_builder = if let Some(override_path) = relaychain.override_wasm {
        //     relay_builder.with_wasm_override(override_path.as_str())
        // } else {
        //     relay_builder
        // };

        relay_builder
            .with_node(|node| node.with_name("alice").with_rpc_port(rpc_port))
            .with_node(|node| node.with_name("bob"))
        // .with_node(|node| node.with_name("charlie"))
        // .with_node(|node| node.with_name("dave"))
    });
    if !paras.is_empty() {
        // TODO: enable for multiple paras
        // let validation_context = Rc::new(RefCell::new(ValidationContext::default()));
        for para in paras {
            // TODO: enable for multiple paras
            // let builder = ParachainConfigBuilder::new(validation_context);
            // let para_config = builder.with_id(1000)
            // .with_chain(para.chain.as_str())
            // .with_default_command(para.cmd.as_str())
            // .with_chain_spec_path(PathBuf::from(para.spec_path.as_str()))
            // .with_default_db_snapshot(PathBuf::from(para.snap_path.as_str()))
            // .with_collator(|c| c.with_name("col-1000"));

            let (chain_spec_path, db_path) = if let Ok(ci_path) = env::var("ZOMBIE_BITE_CI_PATH") {
                let chain_spec_path = PathBuf::from(para.spec_path.as_str());
                let chain_spec_filename = chain_spec_path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .to_string();

                let db_path = PathBuf::from(para.snap_path.as_str());
                let db_path_filename = db_path.file_name().unwrap().to_string_lossy().to_string();

                let new_chain_spec_path =
                    PathBuf::from(&format!("{ci_path}/{}", chain_spec_filename));
                let new_db_path = PathBuf::from(&format!("{ci_path}/{}", db_path_filename));

                tokio::fs::rename(chain_spec_path, &new_chain_spec_path)
                    .await
                    .unwrap();
                tokio::fs::rename(db_path, &new_db_path).await.unwrap();

                (
                    PathBuf::from(format!("./{}", chain_spec_filename)),
                    PathBuf::from(format!("./{}", db_path_filename)),
                )
            } else {
                (
                    PathBuf::from(para.spec_path.as_str()),
                    PathBuf::from(para.snap_path.as_str()),
                )
            };

            let para_rpc_port: u16 = if let Ok(port) = env::var("ZOMBIE_BITE_AH_PORT") {
                port.parse()
                    .expect("env var ZOMBIE_BITE_AH_PORT must be a valid u16")
            } else {
                get_random_port().await
            };

            config = config.with_parachain(|p|{
                let para_builder = p.with_id(1000)
                .with_chain(para.chain.as_str())
                .with_default_command(para.cmd.as_str())
                .with_chain_spec_path(chain_spec_path)
                .with_default_db_snapshot(db_path);

                // we override the runtime in the state now
                // para_builder = if let Some(override_path) = para.override_wasm {
                //     para_builder.with_wasm_override(override_path.as_str())
                // } else {
                //     para_builder
                // };

                para_builder.with_collator(|c|
                    c
                        .with_name("collator")
                        .with_rpc_port(para_rpc_port)
                        .with_args(vec![
                            ("--relay-chain-rpc-urls", format!("ws://127.0.0.1:{rpc_port}").as_str()).into(),
                            ("-l", "aura=debug,runtime=debug,cumulus-consensus=trace,consensus::common=trace,parachain::collation-generation=trace,parachain::collator-protocol=trace,parachain=debug").into(),
                            "--force-authoring".into(),
                            "--discover-local".into(),
                            "--allow-private-ip".into(),
                            "--no-hardware-benchmarks".into(),
                        ])
                )
            })
        }
    }

    let config = if let Some(global_base_dir) = &global_base_dir {
        let fixed_base_dir = global_base_dir.canonicalize().unwrap().join("spawn");
        config.with_global_settings(|global_settings| {
            global_settings.with_base_dir(&fixed_base_dir.to_string_lossy().to_string())
        })
    } else {
        config
    };

    let network_config = config.build().unwrap();

    // spawn the network
    let filesystem = LocalFileSystem;
    let orchestrator = Orchestrator::new(filesystem, provider);
    let toml_config = network_config.dump_to_toml().unwrap();

    if let Ok(ci_path) = env::var("ZOMBIE_BITE_CI_PATH") {
        env::set_current_dir(&ci_path).expect("change current dir to ci should works");
    }


    let network_result = orchestrator.spawn(network_config).await;

    // dump config earlier if we can
    let config_toml_path = if let Ok(ci_path) = env::var("ZOMBIE_BITE_CI_PATH") {
        Some(format!("{ci_path}/config.toml"))
    } else if let Some(base_dir) = &global_base_dir {
        Some(format!("{}/config.toml", base_dir.canonicalize().unwrap().join("spawn").to_string_lossy()))
    } else {
        None
    };

    if let Some(config_toml_path) = &config_toml_path {
        tokio::fs::write(config_toml_path, &toml_config)
            .await
            .unwrap();
    };

    match network_result {
        Ok(network) => {
            if config_toml_path.is_none() {
                // dump config if isn't dumped yet
                let config_toml_path = format!("{}/config.toml", network.base_dir().unwrap());
                tokio::fs::write(config_toml_path, toml_config)
                    .await
                    .unwrap();
                };
            Ok(network)
        },
        Err(e) => Err(e.to_string()),
    }
}

// TODO: enable for multiple paras
// fn add_para(config: NetworkConfigBuilder<WithRelaychain>, para: ChainArtifact) -> NetworkConfigBuilder<WithRelaychain> {
//     let c = config.with_parachain(|p| {
//         p.with_id(1000)
//         .with_chain(para.chain.as_str())
//         .with_default_command(para.cmd.as_str())
//         .with_chain_spec_path(PathBuf::from(para.spec_path.as_str()))
//         .with_default_db_snapshot(PathBuf::from(para.snap_path.as_str()))
//         .with_collator(|c| c.with_name("col-1000"))
//     });
// }

async fn generate_snap(data_path: &str, snap_path: &str) -> Result<(), String> {
    info!("\n📝 Generating snapshot file {snap_path} with data_path {data_path}...");

    let compressed_file = File::create(snap_path).unwrap();
    let mut encoder = GzEncoder::new(compressed_file, Compression::fast());

    let mut archive = Builder::new(&mut encoder);
    archive.append_dir_all("data", data_path).unwrap();
    archive.finish().unwrap();

    info!("✅ generated with path {snap_path}");
    Ok(())
}

async fn generate_chain_spec(
    ns: DynNamespace,
    chain_spec_path: &str,
    cmd: &str,
    chain: &str,
) -> Result<(), String> {
    info!("\n📝 Generating chain-spec file {chain_spec_path} using cmd {cmd} with chain {chain} without bootnodes...");

    let temp_node = ns
        .spawn_node(
            &SpawnNodeOptions::new("temp-polkadot", "bash")
                .args(vec!["-c", "while :; do sleep 60; done"]),
        )
        .await
        .unwrap();

    let cmd_stdout = temp_node
        .run_command(RunCommandOptions::new(cmd).args(vec!["build-spec", "--chain", chain]))
        .await
        .unwrap()
        .unwrap();

    temp_node.destroy().await.unwrap();

    let mut chain_spec_json: serde_json::Value = serde_json::from_str(&cmd_stdout).unwrap();
    chain_spec_json["bootNodes"] = serde_json::Value::Array(vec![]);
    let contents = serde_json::to_string_pretty(&chain_spec_json).unwrap();

    tokio::fs::write(&chain_spec_path, contents).await.unwrap();
    info!("✅ generated with path {chain_spec_path}");

    Ok(())
}

async fn run_doppelganger_node(ns: DynNamespace, base_path: &Path) -> Result<(), String> {
    let data_path = format!("{}/sync_db", &base_path.to_string_lossy());
    let logs_path = format!("{}/sync.log", &base_path.to_string_lossy());
    info!(
        "⛓  Syncing using warp, this could take a while. You can follow the logs with: \n\t
    tail -f {}",
        &logs_path
    );

    let temp_node = ns
        .spawn_node(
            &SpawnNodeOptions::new("temp-doppelganger", "bash")
                .args(vec!["-c", "while :; do sleep 60; done"]),
        )
        .await
        .unwrap();

    let _stdout = temp_node
        .run_command(
            RunCommandOptions::new("bash")
                .args(vec![
                    "-c",
                    format!(
                        "doppelganger -l doppelganger=debug --chain kusama --sync warp -d {} > {} 2>&1",
                        &data_path, &logs_path
                    )
                    .as_str(),
                ])
                // Override rust log for sync
                .env(vec![("RUST_LOG", "")]),
        )
        .await
        .unwrap()
        .unwrap();

    temp_node.destroy().await.unwrap();

    info!("✅ Synced");

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

    #[ignore = "Internal test, require some artifacts"]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_snap() {
        let snap_path = "/tmp/zombie-bite_1726677980197/snap.tgz";
        let demo = generate_snap("/tmp/zombie-bite_1726677980197", snap_path).await;
        // .unwrap();
        println!("{:?}", demo);
        // let _n = spawn(provider, chain_spec_path, snap_path).await.unwrap();
    }

    #[ignore = "Internal test, require some artifacts"]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_spawn() {
        let filesystem = LocalFileSystem;
        let provider = NativeProvider::new(filesystem.clone());
        let r = ChainArtifact {
            cmd: "polkadot".into(),
            chain: "polkadot".into(),
            spec_path: "/tmp/zombie-bite_1730630215147/polkadot-spec.json".into(),
            snap_path: "/tmp/zombie-bite_1730630215147/polkadot-snap.tgz".into(),
            override_wasm: None,
        };

        let p = ChainArtifact {
            cmd: "polkadot-parachain".into(),
            chain: "asset-hub-polkadot".into(),
            spec_path: "/tmp/zombie-bite_1730630215147/asset-hub-polkadot-spec.json".into(),
            snap_path: "/tmp/zombie-bite_1730630215147/asset-hub-polkadot-snap.tgz".into(),
            override_wasm: None,
        };

        let n = spawn(provider, r, vec![p], None).await.unwrap();
        println!("{:?}", n);
        loop {}
    }
}

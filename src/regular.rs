use std::{env, time::Duration};
use tracing::level_filters::LevelFilter;
mod config;

use tracing_subscriber::EnvFilter;
use zombienet_orchestrator::Orchestrator;
use zombienet_provider::NativeProvider;
use zombienet_support::fs::local::LocalFileSystem;
mod cli;

#[tokio::main]
async fn main() {
    // tracing_subscriber::fmt::init();
    // let args: Vec<_> = env::args().collect();
    // println!("{:?}", args);

    // if args.len() < 2 {
    //     panic!(
    //         "Missing argument (network to bite...):
    //     \t zombie-bite <polkadot|kusama> [asset-hub]
    //     "
    //     );
    // }

    // // TODO: move to clap
    // let relay_chain = if args[1] == "polkadot" { config::Relaychain::Polkadot } else { config::Relaychain::Kusama };

    // // TODO: support multiple paras
    // let paras_to: Vec<config::Parachain> = if let Some(paras_to_fork) = args.get(2) {
    //     let mut paras_to = vec![];
    //     for para in paras_to_fork.trim().split(',').into_iter() {
    //         match para {
    //             "asset-hub" => paras_to.push(config::Parachain::AssetHub),
    //             "coretime" => paras_to.push(config::Parachain::Coretime),
    //             _ => {
    //                 warn!("Invalid para {para}, skipping...");
    //             }
    //          }
    //     }
    //     paras_to
    // } else {
    //     vec![]
    // };

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .init();

    let args: Vec<_> = env::args().collect();
    let (relay_chain, paras_to, _) = cli::parse(args);

    let config = config::generate_network_config(&relay_chain, paras_to).unwrap();
    let filesystem = LocalFileSystem;
    let provider = NativeProvider::new(filesystem.clone());

    let orchestrator = Orchestrator::new(filesystem, provider);
    let _n = orchestrator.spawn(config).await.unwrap();

    println!("looping...");

    loop {
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}

use std::{env, time::Duration};
mod config;

use zombienet_orchestrator::Orchestrator;
use zombienet_provider::NativeProvider;
use zombienet_sdk::LocalFileSystem;


#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    let args: Vec<_> = env::args().collect();
    println!("{:?}", args);

    if args.len() < 1 {
        panic!(
            "Missing argument (config.toml):
        \t cargo run --bin cli <config.toml>
        "
        );
    }

    let toml_path  = &args[1];
    let config = zombienet_configuration::NetworkConfig::load_from_toml(&toml_path).unwrap();
    let filesystem = LocalFileSystem;
    let provider = NativeProvider::new(filesystem.clone());
    let orchestrator = Orchestrator::new(filesystem, provider);
    let _n = orchestrator.spawn(config).await.unwrap();

    println!("looping...");

    loop {
        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}
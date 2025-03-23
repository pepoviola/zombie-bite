use std::env;

use tracing::level_filters::LevelFilter;
use tracing_subscriber::EnvFilter;
mod doppelganger;
use doppelganger::doppelganger_inner;

mod cli;
mod config;
mod overrides;
mod sync;
mod utils;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::builder()
                .with_default_directive(LevelFilter::INFO.into())
                .from_env_lossy(),
        )
        .init();

    let args: Vec<_> = env::args().collect();
    let (relay_chain, paras_to, _bite_method) = cli::parse(args);
    doppelganger_inner(relay_chain, paras_to).await
}

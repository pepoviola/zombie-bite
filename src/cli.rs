use crate::config::{Relaychain, Parachain};

pub fn parse(args: Vec<String>) -> (Relaychain, Vec<Parachain>) {

   // TODO: move to clap
   let relay_chain = match args[1].as_str() {
        "polkadot" => Relaychain::Polkadot,
        "kusama" => Relaychain::Kusama,
        _ => {
            panic!("Invalid network, should be one of 'polkadot, kusama'");
        }
    };

    // TODO: support multiple paras
    let paras_to: Vec<Parachain> = if let Some(paras_to_fork) = args.get(2) {
        let mut paras_to = vec![];
        for para in paras_to_fork.trim().split(',').into_iter() {
            match para {
                "asset-hub" => paras_to.push(Parachain::AssetHub),
                "coretime" => paras_to.push(Parachain::Coretime),
                "people" => paras_to.push(Parachain::People),
                _ => {
                    println!("Invalid para {para}, skipping...");
                }
            }
        }
        paras_to
    } else {
        vec![]
    };

    println!("{:?}",paras_to);

(relay_chain, paras_to)
}
use crate::config::{BiteMethod, Parachain, Relaychain};

pub fn parse(args: Vec<String>) -> (Relaychain, Vec<Parachain>, BiteMethod) {
    println!("{:?}", args);
    let Some(relay) = args.get(1) else {
        panic!("Relaychain argument must be present. (Either polkadot or kusama");
    };

    // TODO: move to clap
    let parts: Vec<&str> = relay.split(':').collect();
    let relaychain = parts.first().expect("relaychain should be valid");
    let wasm_overrides = parts.get(1).map(|path| path.to_string());
    // if let Some(path) = parts.get(1) {
    //     Some(path.to_string())
    // } else {
    //     None
    // };

    let relay_chain = match *relaychain {
        "polkadot" => Relaychain::Polkadot(wasm_overrides),
        "kusama" => Relaychain::Kusama(wasm_overrides),
        _ => {
            let msg =
                format!("Invalid network, should be one of 'polkadot, kusama', you pass: {relay}");
            panic!("{msg}");
        }
    };

    let mut bite_method = BiteMethod::DoppelGanger;

    // TODO: support multiple paras
    let paras_to: Vec<Parachain> = if let Some(paras_to_fork) = args.get(2) {
        // Allow to not use any para
        if paras_to_fork == "fork-off" || paras_to_fork == "doppelganger" {
            bite_method = paras_to_fork.into();
            vec![]
        } else {
            let mut paras_to = vec![];
            for para in paras_to_fork.trim().split(',') {
                let parts: Vec<&str> = para.split(':').collect();
                let parachain = parts.first().expect("chain should be valid");
                let wasm_overrides = parts.get(1).map(|path| path.to_string());
                // if let Some(path) = parts.get(1) {
                //     Some(path.to_string())
                // } else {
                //     None
                // };

                match *parachain {
                    "asset-hub" => paras_to.push(Parachain::AssetHub(wasm_overrides)),
                    //"coretime" => paras_to.push(Parachain::Coretime),
                    // "people" => paras_to.push(Parachain::People),
                    _ => {
                        println!("Invalid para {para}, skipping...");
                    }
                }
            }
            paras_to
        }
    } else {
        vec![]
    };

    println!("rc: {:?}, paras: {:?}", relay_chain, paras_to);

    let bite_method = if let Some(method) = args.get(3) {
        method.into()
    } else {
        bite_method
    };

    (relay_chain, paras_to, bite_method)
}

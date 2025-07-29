#![allow(dead_code)]
// TODO: don't allow dead_code

use std::env;

use zombienet_configuration::{NetworkConfig, NetworkConfigBuilder};

#[derive(Debug, PartialEq)]
pub enum BiteMethod {
    DoppelGanger,
    Fork,
}

impl<T> From<T> for BiteMethod
where
    T: AsRef<str>,
{
    fn from(s: T) -> Self {
        if s.as_ref() == "fork-off" {
            BiteMethod::Fork
        } else {
            BiteMethod::DoppelGanger
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Context {
    Relaychain,
    Parachain,
}

impl Context {
    pub fn cmd(&self) -> String {
        String::from(if *self == Context::Relaychain {
            "polkadot"
        } else {
            "polkadot-parachain"
        })
    }

    pub fn doppelganger_cmd(&self) -> String {
        String::from(if *self == Context::Relaychain {
            "doppelganger"
        } else {
            "doppelganger-parachain"
        })
    }
}

type MaybeWasmOverridePath = Option<String>;

#[derive(Debug, PartialEq)]
pub enum Relaychain {
    Polkadot(MaybeWasmOverridePath),
    Kusama(MaybeWasmOverridePath),
    Westend(MaybeWasmOverridePath),
    Paseo(MaybeWasmOverridePath),
}

impl Relaychain {
    pub fn as_local_chain_string(&self) -> String {
        String::from(match self {
            Relaychain::Polkadot(_) => "polkadot-local",
            Relaychain::Kusama(_) => "kusama-local",
            Relaychain::Westend(_) => "westend-local",
            Relaychain::Paseo(_) => "paseo-local",
        })
    }

    pub fn as_chain_string(&self) -> String {
        String::from(match self {
            Relaychain::Polkadot(_) => "polkadot",
            Relaychain::Kusama(_) => "kusama",
            Relaychain::Westend(_) => "westend",
            Relaychain::Paseo(_) => "paseo",
        })
    }

    // TODO: make this endpoints configurables
    pub fn sync_endpoint(&self) -> String {
        String::from(match self {
            Relaychain::Polkadot(_) => "wss://polkadot-rpc.dwellir.com",
            Relaychain::Kusama(_) => "wss://kusama-rpc.dwellir.com",
            Relaychain::Westend(_) => "wss://westend-rpc.polkadot.io",
            Relaychain::Paseo(_) => "wss://paseo-rpc.dwellir.com",
        })
    }

    pub fn context(&self) -> Context {
        Context::Relaychain
    }

    pub fn wasm_overrides(&self) -> Option<&str> {
        match self {
            Relaychain::Kusama(x)
            | Relaychain::Polkadot(x)
            | Relaychain::Westend(x)
            | Relaychain::Paseo(x) => x.as_deref(),
        }
    }

    pub fn epoch_duration(&self) -> u64 {
        match self {
            Relaychain::Paseo(_) => 600,
            _ => 2400,
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum Parachain {
    AssetHub(MaybeWasmOverridePath),
    Coretime(MaybeWasmOverridePath),
    People(MaybeWasmOverridePath),
    // Bridge
}

impl Parachain {
    pub fn as_local_chain_string(&self, relay_part: &str) -> String {
        let para_part = match self {
            Parachain::AssetHub(_) => "asset-hub",
            Parachain::Coretime(_) => "coretime",
            Parachain::People(_) => "people",
        };

        format!("{para_part}-{relay_part}-local")
    }

    pub fn as_chain_string(&self, relay_part: &str) -> String {
        let para_part = match self {
            Parachain::AssetHub(_) => "asset-hub",
            Parachain::Coretime(_) => "coretime",
            Parachain::People(_) => "people",
        };

        format!("{para_part}-{relay_part}")
    }

    pub fn context(&self) -> Context {
        Context::Parachain
    }

    pub fn id(&self) -> u32 {
        match self {
            Parachain::AssetHub(_) => 1000,
            Parachain::Coretime(_) => 1005,
            Parachain::People(_) => 1001,
        }
    }

    pub fn wasm_overrides(&self) -> Option<&str> {
        match self {
            Parachain::AssetHub(x) | Parachain::Coretime(x) | Parachain::People(x) => x.as_deref(),
        }
    }
}

// Chain generator command template
const CMD_TPL: &str = "chain-spec-generator {{chainName}}";

pub const DEFAULT_CHAIN_SPEC_TPL_COMMAND: &str =
    "{{mainCommand}} build-spec --chain {{chainName}} {{disableBootnodes}}";

// Relaychain nodes
const ALICE: &str = "alice";
const BOB: &str = "bob";
const CHARLIE: &str = "charlie";
const DAVE: &str = "dave";

pub fn generate_network_config(
    network: &Relaychain,
    paras: Vec<Parachain>,
) -> Result<NetworkConfig, anyhow::Error> {
    println!("paras: {:?}", paras);
    // TODO: integrate k8s/docker
    // let images = environment::get_images_from_env();
    let relay_chain = network.as_local_chain_string();
    let relay_context = Context::Relaychain;
    let para_context = Context::Parachain;

    let chain_spec_cmd = match network {
        Relaychain::Polkadot(_) | Relaychain::Kusama(_) => CMD_TPL,
        Relaychain::Westend(_) | Relaychain::Paseo(_) => DEFAULT_CHAIN_SPEC_TPL_COMMAND,
    };

    let network_builder = NetworkConfigBuilder::new().with_relaychain(|r| {
        let relaychain_builder = r
            .with_chain(relay_chain.as_str())
            .with_default_command(relay_context.cmd().as_str())
            .with_chain_spec_command(chain_spec_cmd)
            .chain_spec_command_is_local(true)
            // .with_default_args(vec![("-l", "babe=debug,grandpa=debug,runtime=debug,parachain::=debug,sub-authority-discovery=trace").into()])
            .with_default_args(vec![("-l", "runtime=trace").into()]);

        let relaychain_builder = if let Ok(port) = env::var("ZOMBIE_BITE_RC_PORT") {
            let rpc_port = port
                .parse()
                .expect("env var ZOMBIE_BITE_RC_PORT must be a valid u16");
            relaychain_builder.with_node(|node| node.with_name(ALICE).with_rpc_port(rpc_port))
        } else {
            relaychain_builder.with_node(|node| node.with_name(ALICE))
        };

        // .with_node(|node| node.with_name(ALICE))
        relaychain_builder.with_node(|node| node.with_name(BOB))
        // .with_node(|node| node.with_name(CHARLIE))
        // .with_node(|node| node.with_name(DAVE))
    });

    let network_builder = paras.iter().fold(network_builder, |builder, para| {
        println!("para: {:?}", para);
        let (chain_part, id) = match para {
            Parachain::AssetHub(_) => ("asset-hub", para.id()),
            Parachain::Coretime(_) => ("coretime", para.id()),
            Parachain::People(_) => ("people", para.id()),
        };
        let chain = format!("{}-{}",chain_part, relay_chain);

        builder.with_parachain(|p| {
            p.with_id(id)
                .with_default_command(para_context.cmd().as_str())
                .with_chain(chain.as_str())
                .with_chain_spec_command(chain_spec_cmd)
                .with_collator(|c| {
                    // TODO: use single collator for now
                    // c.with_name(&format!("col-{}",id))
                    let col_builder = c.with_name("collator")
                    .with_args(vec![
                        ("-l", "aura=debug,runtime=trace,cumulus-consensus=trace,consensus::common=trace,parachain::collation-generation=trace,parachain::collator-protocol=trace,parachain=debug,basic-authorship=trace").into(),
                        "--force-authoring".into()
                    ]);
                    if let Ok(port) = env::var("ZOMBIE_BITE_AH_PORT") {
                        let rpc_port = port.parse().expect("env var ZOMBIE_BITE_AH_PORT must be a valid u16");
                        col_builder.with_rpc_port(rpc_port)
                    } else {
                        col_builder
                    }
                })
        })
    });

    let config = network_builder.build().map_err(|errs| {
        let e = errs
            .iter()
            .fold("".to_string(), |memo, err| format!("{memo} \n {err}"));
        anyhow::anyhow!(e)
    })?;

    Ok(config)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn config_ok() {
        let config = generate_network_config(&Relaychain::Kusama(None), vec![]).unwrap();
        assert_eq!(0, config.parachains().len());
    }

    #[test]
    fn config_with_para_ok() {
        let config =
            generate_network_config(&Relaychain::Kusama(None), vec![Parachain::Coretime(None)])
                .unwrap();
        let parachain = config.parachains().first().unwrap().chain().unwrap();
        assert_eq!(parachain.as_str(), "coretime-kusama-local");
    }

    #[tokio::test]
    async fn spec() {
        let config =
            generate_network_config(&Relaychain::Kusama(None), vec![Parachain::AssetHub(None)])
                .unwrap();
        println!("config: {:#?}", config);
        let spec = zombienet_orchestrator::NetworkSpec::from_config(&config)
            .await
            .unwrap();

        println!("{:#?}", spec);
    }
}

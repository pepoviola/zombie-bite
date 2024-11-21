use zombienet_configuration::{NetworkConfig, NetworkConfigBuilder};

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
}

#[derive(Debug, PartialEq)]
pub enum Relaychain {
    Polkadot,
    Kusama,
    Rococo,
}

impl Relaychain {
    pub fn as_local_chain_string(&self) -> String {
        String::from(match self {
            Relaychain::Polkadot => "polkadot-local",
            Relaychain::Kusama => "kusama-local",
            Relaychain::Rococo => "rococo-local",
        })
    }

    pub fn as_chain_string(&self) -> String {
        String::from(match self {
            Relaychain::Polkadot => "polkadot",
            Relaychain::Kusama => "kusama",
            Relaychain::Rococo => "rococo",
        })
    }

    pub fn context(&self) -> Context {
        Context::Relaychain
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum Parachain {
    AssetHub,
    Coretime,
    People,
    // Bridge
}


impl Parachain {
    pub fn as_local_chain_string(&self, relay_part: &str) -> String {
        let para_part = match self {
            Parachain::AssetHub => "asset-hub",
            Parachain::Coretime => "coretime",
            Parachain::People => "people",
        };

        format!("{para_part}-{relay_part}-local")
    }

    pub fn as_chain_string(&self, relay_part: &str) -> String {
        let para_part = match self {
            Parachain::AssetHub => "asset-hub",
            Parachain::Coretime => "coretime",
            Parachain::People => "people",
        };

        format!("{para_part}-{relay_part}")
    }

    pub fn context(&self) -> Context {
        Context::Parachain
    }

    pub fn id(&self) -> u32 {
        match self {
            Parachain::AssetHub => 1000,
            Parachain::Coretime => 1005,
            Parachain::People => 1001,
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

    let chain_spec_cmd = if *network == Relaychain::Rococo {
        DEFAULT_CHAIN_SPEC_TPL_COMMAND
    } else {
        CMD_TPL
    };

    let network_builder = NetworkConfigBuilder::new().with_relaychain(|r| {
        r.with_chain(relay_chain.as_str())
            .with_default_command(relay_context.cmd().as_str())
            .with_chain_spec_command(chain_spec_cmd)
            .chain_spec_command_is_local(true)
            .with_default_args(vec![("-l", "babe=debug,grandpa=debugruntime=debug,parachain::=debug,sub-authority-discovery=trace").into()])
            .with_node(|node| node.with_name(ALICE))
            .with_node(|node| node.with_name(BOB))
            .with_node(|node| node.with_name(CHARLIE))
            .with_node(|node| node.with_name(DAVE))
    });

    let network_builder = paras.iter().fold(network_builder, |builder, para| {
        println!("para: {:?}", para);
        let (chain_part, id) = match para {
            Parachain::AssetHub => ("asset-hub", para.id()),
            Parachain::Coretime => ("coretime", para.id()),
            Parachain::People => ("people", para.id()),
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
                    c.with_name("collator")
                    .with_args(vec![
                        ("-l", "aura=debug,runtime=debug,cumulus-consensus=trace,consensus::common=trace,parachain::collation-generation=trace,parachain::collator-protocol=trace,parachain=debug,sub-authority-discovery=trace").into(),
                        "--force-authoring".into()
                        ])
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
        let config = generate_network_config(&Relaychain::Kusama, vec![]).unwrap();
        assert_eq!(0, config.parachains().len());
    }

    #[test]
    fn config_with_para_ok() {
        let config =
            generate_network_config(&Relaychain::Kusama, vec![Parachain::Coretime]).unwrap();
        let parachain = config.parachains().first().unwrap().chain().unwrap();
        assert_eq!(parachain.as_str(), "coretime-kusama-local");
    }

    #[tokio::test]
    async fn spec() {
        let config =
            generate_network_config(&Relaychain::Kusama, vec![Parachain::AssetHub]).unwrap();
        println!("config: {:#?}", config);
        let spec = zombienet_orchestrator::NetworkSpec::from_config(&config)
            .await
            .unwrap();

        println!("{:#?}", spec);
    }
}

use zombienet_configuration::{NetworkConfig, NetworkConfigBuilder, RegistrationStrategy};

#[derive(Debug, PartialEq)]
pub enum Context {
    Relaychain,
    Parachain
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
    Kusama
}

impl Relaychain {
    pub fn as_local_chain_string(&self) -> String {
        String::from(if *self == Relaychain::Polkadot {
            "polkadot-local"
        } else {
            "kusama-local"
        })
    }

    pub fn as_chain_string(&self) -> String {
        String::from(if *self == Relaychain::Polkadot {
            "polkadot"
        } else {
            "kusama"
        })
    }

    pub fn context(&self) -> Context {
        Context::Relaychain
    }
}

#[derive(Debug, PartialEq)]
pub enum Parachain {
    AssetHub,
    Coretime,
    // People
    // Bridge
}

impl Parachain {
    pub fn as_local_chain_string(&self, relay_part: &str) -> String {
        let para_part = match self {
            Parachain::AssetHub => "asset-hub",
            Parachain::Coretime => "coretime",
        };

        format!("{para_part}-{relay_part}-local")
    }

    pub fn as_chain_string(&self, relay_part: &str) -> String {
        let para_part = match self {
            Parachain::AssetHub => "asset-hub",
            Parachain::Coretime => "coretime",
        };

        format!("{para_part}-{relay_part}")
    }

    pub fn context(&self) -> Context {
        Context::Parachain
    }
}


// Chain generator command template
const CMD_TPL: &str = "chain-spec-generator {{chainName}}";

// Relaychain nodes
const ALICE: &str = "alice";
const BOB: &str = "bob";
const CHARLIE: &str = "charlie";
const DAVE: &str = "dave";


pub fn generate_network_config(network: &Relaychain, paras: Vec<Parachain>) -> Result<NetworkConfig, anyhow::Error> {
    println!("paras: {:?}", paras);
	// TODO: integrate k8s/docker
    // let images = environment::get_images_from_env();
    let relay_chain = network.as_local_chain_string();

	let network_builder = NetworkConfigBuilder::new()
		.with_relaychain(|r| {
			r.with_chain(relay_chain.as_str())
				.with_default_command("polkadot")
				// .with_default_image(images.polkadot.as_str())
				.with_chain_spec_command(CMD_TPL)
				.chain_spec_command_is_local(true)
				.with_node(|node| node.with_name(ALICE))
				.with_node(|node| node.with_name(BOB))
                .with_node(|node| node.with_name(CHARLIE))
                .with_node(|node| node.with_name(DAVE))
		});


    let network_builder = paras.iter().fold(network_builder, |builder, para| {
        let (chain_part, id) = match para {
            Parachain::AssetHub => ("asset-hub", 1000_u32),
            Parachain::Coretime => ("coretime", 1005_u32),
        };
        let chain = format!("{}-{}",chain_part, relay_chain);
        builder.with_parachain(|p| {
            p.with_id(id)
                .with_chain(chain.as_str())
                .with_chain_spec_command(CMD_TPL)
                .with_registration_strategy(RegistrationStrategy::Manual)
                .with_collator(|c| {
                    c.with_name(&format!("collator-{}",id))
                })
        })
    });

    let config = network_builder
    .build()
    .map_err(|errs| {
        let e = errs.iter().fold("".to_string(), |memo, err| format!("{memo} \n {err}"));
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
        let config = generate_network_config(&Relaychain::Kusama, vec![Parachain::Coretime]).unwrap();
        let parachain = config.parachains().first().unwrap().chain().unwrap();
        assert_eq!(parachain.as_str(), "coretime-kusama-local");
    }
}
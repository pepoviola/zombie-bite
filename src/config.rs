#![allow(dead_code)]
// TODO: don't allow dead_code

use serde::{Deserialize, Serialize};
use std::env;

use zombienet_configuration::{NetworkConfig, NetworkConfigBuilder};
const BITE: &str = "bite";
const SPAWN: &str = "spawn";
const POST: &str = "post";
const AFTER: &str = "after";
const DEBUG: &str = "debug";

// `--state-pruning` config flag (two days +1 by default)
pub const STATE_PRUNING: &str = "28801";
pub fn get_state_pruning_config() -> String {
    env::var("ZOMBIE_BITE_STATE_PRUNING").unwrap_or_else(|_| STATE_PRUNING.to_string())
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Step {
    /// Initial step
    Bite,
    /// Spawn from `bite` directory
    Spawn,
    /// Spawn from `spawn` directory
    Post,
    /// Spawn from `post` directory
    After,
}

impl Step {
    pub fn dir(&self) -> String {
        match self {
            Step::Bite => String::from(BITE),
            Step::Spawn => String::from(SPAWN),
            Step::Post => String::from(POST),
            Step::After => String::from(AFTER),
        }
    }

    pub fn dir_debug(&self) -> String {
        match self {
            Step::Bite => format!("{BITE}-{DEBUG}"),
            Step::Spawn => format!("{SPAWN}-{DEBUG}"),
            Step::Post => format!("{POST}-{DEBUG}"),
            Step::After => String::from("{AFTER}-{DEBUG}"),
        }
    }

    pub fn dir_from(&self) -> String {
        match self {
            Step::Bite => String::from(""), // emtpy since is initial step
            Step::Spawn => String::from(BITE),
            Step::Post => String::from(SPAWN),
            Step::After => String::from(POST),
        }
    }

    pub fn next(&self) -> Option<String> {
        match self {
            Step::Bite => Some(String::from(SPAWN)),
            Step::Spawn => Some(String::from(POST)),
            Step::Post => Some(String::from(AFTER)),
            Step::After => None, // emtpy since is the last step
        }
    }
}

impl From<String> for Step {
    fn from(value: String) -> Self {
        match value.to_ascii_lowercase().as_str() {
            "post" => Step::Post,
            "spawn" => Step::Spawn,
            "after" => Step::After,
            _ => Step::Bite,
        }
    }
}
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
type MaybeSyncUrl = Option<String>;

#[derive(Debug, PartialEq)]
pub enum Relaychain {
    Polkadot {
        maybe_override: MaybeWasmOverridePath,
        maybe_sync_url: MaybeSyncUrl,
    },
    Kusama {
        maybe_override: MaybeWasmOverridePath,
        maybe_sync_url: MaybeSyncUrl,
    },

    Paseo {
        maybe_override: MaybeWasmOverridePath,
        maybe_sync_url: MaybeSyncUrl,
    },
}

impl Relaychain {
    pub fn new(network: impl AsRef<str>) -> Self {
        match network.as_ref() {
            "kusama" => Self::Kusama {
                maybe_override: None,
                maybe_sync_url: None,
            },
            "paseo" => Self::Paseo {
                maybe_override: None,
                maybe_sync_url: None,
            },
            _ => Self::Polkadot {
                maybe_override: None,
                maybe_sync_url: None,
            },
        }
    }

    pub fn new_with_values(
        network: impl AsRef<str>,
        maybe_override: MaybeWasmOverridePath,
        maybe_sync_url: MaybeSyncUrl,
    ) -> Self {
        match network.as_ref() {
            "kusama" => Self::Kusama {
                maybe_override,
                maybe_sync_url,
            },
            "paseo" => Self::Paseo {
                maybe_override,
                maybe_sync_url,
            },
            _ => Self::Polkadot {
                maybe_override,
                maybe_sync_url,
            },
        }
    }

    pub fn as_local_chain_string(&self) -> String {
        String::from(match self {
            Relaychain::Polkadot { .. } => "polkadot-local",
            Relaychain::Kusama { .. } => "kusama-local",
            Relaychain::Paseo { .. } => "paseo-local",
        })
    }

    pub fn as_chain_string(&self) -> String {
        String::from(match self {
            Relaychain::Polkadot { .. } => "polkadot",
            Relaychain::Kusama { .. } => "kusama",
            Relaychain::Paseo { .. } => "paseo",
        })
    }

    // TODO: make this endpoints configurables
    pub fn sync_endpoint(&self) -> String {
        String::from(match self {
            Relaychain::Polkadot { .. } => "wss://polkadot-rpc.dwellir.com",
            Relaychain::Kusama { .. } => "wss://kusama-rpc.dwellir.com",
            Relaychain::Paseo { .. } => "wss://paseo-rpc.dwellir.com",
        })
    }

    pub fn context(&self) -> Context {
        Context::Relaychain
    }

    pub fn wasm_overrides(&self) -> Option<&str> {
        match self {
            Relaychain::Kusama { maybe_override, .. }
            | Relaychain::Polkadot { maybe_override, .. }
            | Relaychain::Paseo { maybe_override, .. } => maybe_override.as_deref(),
        }
    }

    pub fn epoch_duration(&self) -> u64 {
        match self {
            Relaychain::Paseo { .. } => 600,
            Relaychain::Kusama { .. } => 600,
            _ => 2400,
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub enum Parachain {
    AssetHub(MaybeWasmOverridePath),
    Coretime(MaybeWasmOverridePath),
    People(MaybeWasmOverridePath),
    BridgeHub(MaybeWasmOverridePath),
}

impl Parachain {
    pub fn as_local_chain_string(&self, relay_part: &str) -> String {
        let para_part = match self {
            Parachain::AssetHub(_) => "asset-hub",
            Parachain::Coretime(_) => "coretime",
            Parachain::People(_) => "people",
            Parachain::BridgeHub(_) => "bridge-hub",
        };

        format!("{para_part}-{relay_part}-local")
    }

    pub fn as_chain_string(&self, relay_part: &str) -> String {
        let para_part = match self {
            Parachain::AssetHub(_) => "asset-hub",
            Parachain::Coretime(_) => "coretime",
            Parachain::People(_) => "people",
            Parachain::BridgeHub(_) => "bridge-hub",
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
            Parachain::BridgeHub(_) => 1002,
        }
    }

    pub fn wasm_overrides(&self) -> Option<&str> {
        match self {
            Parachain::AssetHub(x)
            | Parachain::Coretime(x)
            | Parachain::People(x)
            | Parachain::BridgeHub(x) => x.as_deref(),
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
        Relaychain::Polkadot { .. } | Relaychain::Kusama { .. } => CMD_TPL,
        Relaychain::Paseo { .. } => DEFAULT_CHAIN_SPEC_TPL_COMMAND,
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
            Parachain::BridgeHub(_) => ("bridge-hub", para.id()),
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

// Configuration file structures
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct ZombieBiteConfig {
    pub relaychain: RelaychainConfig,
    pub parachains: Option<Vec<ParachainConfig>>,
    pub base_path: Option<String>,
    pub and_spawn: Option<bool>,
    pub with_monitor: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct RelaychainConfig {
    pub network: String, // polkadot, kusama, paseo
    pub runtime_override: Option<String>,
    pub sync_url: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct ParachainConfig {
    #[serde(rename = "type")]
    pub parachain_type: String, // asset-hub, coretime, people, bridge-hub
    pub runtime_override: Option<String>,
    pub enabled: Option<bool>, // default true
}

impl ParachainConfig {
    pub fn to_parachain(&self) -> Option<Parachain> {
        if self.enabled.unwrap_or(true) {
            match self.parachain_type.as_str() {
                "asset-hub" => Some(Parachain::AssetHub(self.runtime_override.clone())),
                "coretime" => Some(Parachain::Coretime(self.runtime_override.clone())),
                "people" => Some(Parachain::People(self.runtime_override.clone())),
                "bridge-hub" => Some(Parachain::BridgeHub(self.runtime_override.clone())),
                _ => None,
            }
        } else {
            None
        }
    }
}

impl ZombieBiteConfig {
    pub fn from_file(path: &str) -> Result<Self, anyhow::Error> {
        let contents = std::fs::read_to_string(path)?;
        let config: ZombieBiteConfig = toml::from_str(&contents)?;
        Ok(config)
    }

    pub fn get_relaychain(&self) -> Relaychain {
        Relaychain::new_with_values(
            &self.relaychain.network,
            self.relaychain.runtime_override.clone(),
            self.relaychain.sync_url.clone(),
        )
    }

    pub fn get_parachains(&self) -> Vec<Parachain> {
        self.parachains
            .as_ref()
            .map(|paras| paras.iter().filter_map(|p| p.to_parachain()).collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn config_ok() {
        let config = generate_network_config(&Relaychain::new("kusama"), vec![]).unwrap();
        assert_eq!(0, config.parachains().len());
    }

    #[test]
    fn config_with_para_ok() {
        let config =
            generate_network_config(&Relaychain::new("kusama"), vec![Parachain::AssetHub(None)])
                .unwrap();
        let parachain = config.parachains().first().unwrap().chain().unwrap();
        assert_eq!(parachain.as_str(), "asset-hub-kusama-local");
    }

    #[tokio::test]
    async fn spec() {
        let config =
            generate_network_config(&Relaychain::new("kusama"), vec![Parachain::AssetHub(None)])
                .unwrap();
        println!("config: {:#?}", config);
        let spec = zombienet_orchestrator::NetworkSpec::from_config(&config)
            .await
            .unwrap();

        println!("{:#?}", spec);
    }

    #[test]
    fn parachain_config_enabled_defaults_to_true() {
        let config = ParachainConfig {
            parachain_type: "asset-hub".to_string(),
            runtime_override: None,
            enabled: None, // Not specified
        };

        assert!(config.to_parachain().is_some());
        match config.to_parachain().unwrap() {
            Parachain::AssetHub(_) => {}
            _ => panic!("Expected AssetHub parachain"),
        }
    }

    #[test]
    fn parachain_config_explicitly_enabled() {
        let config = ParachainConfig {
            parachain_type: "coretime".to_string(),
            runtime_override: None,
            enabled: Some(true),
        };

        assert!(config.to_parachain().is_some());
        match config.to_parachain().unwrap() {
            Parachain::Coretime(_) => {}
            _ => panic!("Expected Coretime parachain"),
        }
    }

    #[test]
    fn parachain_config_explicitly_disabled() {
        let config = ParachainConfig {
            parachain_type: "people".to_string(),
            runtime_override: None,
            enabled: Some(false),
        };

        assert!(config.to_parachain().is_none());
    }

    #[test]
    fn parachain_config_with_runtime_override() {
        let override_path = "/path/to/runtime.wasm".to_string();
        let config = ParachainConfig {
            parachain_type: "bridge-hub".to_string(),
            runtime_override: Some(override_path.clone()),
            enabled: Some(true),
        };

        let parachain = config.to_parachain().unwrap();
        match parachain {
            Parachain::BridgeHub(Some(path)) => assert_eq!(path, override_path),
            _ => panic!("Expected BridgeHub with runtime override"),
        }
    }

    #[test]
    fn parachain_config_invalid_type() {
        let config = ParachainConfig {
            parachain_type: "invalid-chain".to_string(),
            runtime_override: None,
            enabled: Some(true),
        };

        assert!(config.to_parachain().is_none());
    }

    #[test]
    fn all_parachain_types_supported() {
        let types = vec!["asset-hub", "coretime", "people", "bridge-hub"];

        for parachain_type in types {
            let config = ParachainConfig {
                parachain_type: parachain_type.to_string(),
                runtime_override: None,
                enabled: Some(true),
            };

            assert!(
                config.to_parachain().is_some(),
                "Failed for type: {}",
                parachain_type
            );
        }
    }

    #[test]
    fn parachain_ids_are_correct() {
        assert_eq!(Parachain::AssetHub(None).id(), 1000);
        assert_eq!(Parachain::Coretime(None).id(), 1005);
        assert_eq!(Parachain::People(None).id(), 1001);
        assert_eq!(Parachain::BridgeHub(None).id(), 1002);
    }

    #[test]
    fn parachain_chain_strings() {
        let relay = "polkadot";

        assert_eq!(
            Parachain::AssetHub(None).as_chain_string(relay),
            "asset-hub-polkadot"
        );
        assert_eq!(
            Parachain::Coretime(None).as_chain_string(relay),
            "coretime-polkadot"
        );
        assert_eq!(
            Parachain::People(None).as_chain_string(relay),
            "people-polkadot"
        );
        assert_eq!(
            Parachain::BridgeHub(None).as_chain_string(relay),
            "bridge-hub-polkadot"
        );
    }

    #[test]
    fn parachain_local_chain_strings() {
        let relay = "kusama";

        assert_eq!(
            Parachain::AssetHub(None).as_local_chain_string(relay),
            "asset-hub-kusama-local"
        );
        assert_eq!(
            Parachain::Coretime(None).as_local_chain_string(relay),
            "coretime-kusama-local"
        );
        assert_eq!(
            Parachain::People(None).as_local_chain_string(relay),
            "people-kusama-local"
        );
        assert_eq!(
            Parachain::BridgeHub(None).as_local_chain_string(relay),
            "bridge-hub-kusama-local"
        );
    }

    #[test]
    fn relaychain_creation() {
        let polkadot = Relaychain::new("polkadot");
        assert_eq!(polkadot.as_chain_string(), "polkadot");

        let kusama = Relaychain::new("kusama");
        assert_eq!(kusama.as_chain_string(), "kusama");

        let paseo = Relaychain::new("paseo");
        assert_eq!(paseo.as_chain_string(), "paseo");

        // Unknown defaults to polkadot
        let unknown = Relaychain::new("unknown");
        assert_eq!(unknown.as_chain_string(), "polkadot");
    }

    #[test]
    fn relaychain_with_overrides() {
        let runtime_path = Some("/path/to/runtime.wasm".to_string());
        let sync_url = Some("wss://custom-rpc.example.com".to_string());

        let relaychain =
            Relaychain::new_with_values("kusama", runtime_path.clone(), sync_url.clone());

        assert_eq!(relaychain.wasm_overrides(), runtime_path.as_deref());
        match relaychain {
            Relaychain::Kusama { maybe_sync_url, .. } => assert_eq!(maybe_sync_url, sync_url),
            _ => panic!("Expected Kusama relaychain"),
        }
    }

    #[test]
    fn relaychain_epoch_durations() {
        assert_eq!(Relaychain::new("polkadot").epoch_duration(), 2400);
        assert_eq!(Relaychain::new("kusama").epoch_duration(), 600);
        assert_eq!(Relaychain::new("paseo").epoch_duration(), 600);
    }

    #[test]
    fn generate_config_with_all_parachains() {
        let relaychain = Relaychain::new("polkadot");
        let parachains = vec![
            Parachain::AssetHub(None),
            Parachain::Coretime(None),
            Parachain::People(None),
            Parachain::BridgeHub(None),
        ];

        let config = generate_network_config(&relaychain, parachains).unwrap();
        assert_eq!(config.parachains().len(), 4);
    }

    #[test]
    fn generate_config_with_runtime_overrides() {
        let relaychain = Relaychain::new_with_values(
            "kusama",
            Some("/path/to/relay_runtime.wasm".to_string()),
            None,
        );
        let parachains = vec![Parachain::AssetHub(Some(
            "/path/to/ah_runtime.wasm".to_string(),
        ))];

        let config = generate_network_config(&relaychain, parachains).unwrap();
        assert_eq!(config.parachains().len(), 1);
    }

    #[test]
    fn zombie_bite_config_get_parachains_empty() {
        let config = ZombieBiteConfig {
            relaychain: RelaychainConfig {
                network: "polkadot".to_string(),
                runtime_override: None,
                sync_url: None,
            },
            parachains: None,
            base_path: None,
            and_spawn: None,
            with_monitor: None,
        };

        assert_eq!(config.get_parachains().len(), 0);
    }

    #[test]
    fn zombie_bite_config_get_parachains_with_enabled_disabled_mix() {
        let config = ZombieBiteConfig {
            relaychain: RelaychainConfig {
                network: "kusama".to_string(),
                runtime_override: None,
                sync_url: None,
            },
            parachains: Some(vec![
                ParachainConfig {
                    parachain_type: "asset-hub".to_string(),
                    runtime_override: None,
                    enabled: Some(true),
                },
                ParachainConfig {
                    parachain_type: "coretime".to_string(),
                    runtime_override: None,
                    enabled: Some(false), // Disabled
                },
                ParachainConfig {
                    parachain_type: "people".to_string(),
                    runtime_override: None,
                    enabled: None, // Defaults to true
                },
            ]),
            base_path: None,
            and_spawn: None,
            with_monitor: None,
        };

        let parachains = config.get_parachains();
        assert_eq!(parachains.len(), 2); // Only asset-hub and people should be enabled

        // Check that the right parachains are included
        let para_ids: Vec<u32> = parachains.iter().map(|p| p.id()).collect();
        assert!(para_ids.contains(&1000)); // asset-hub
        assert!(para_ids.contains(&1001)); // people
        assert!(!para_ids.contains(&1005)); // coretime (disabled)
    }

    #[test]
    fn step_enum_conversion() {
        assert_eq!(Step::from("bite".to_string()), Step::Bite);
        assert_eq!(Step::from("spawn".to_string()), Step::Spawn);
        assert_eq!(Step::from("post".to_string()), Step::Post);
        assert_eq!(Step::from("after".to_string()), Step::After);
        assert_eq!(Step::from("SPAWN".to_string()), Step::Spawn); // Case insensitive
        assert_eq!(Step::from("unknown".to_string()), Step::Bite); // Unknown defaults to Bite
    }

    #[test]
    fn step_directories() {
        assert_eq!(Step::Bite.dir(), "bite");
        assert_eq!(Step::Spawn.dir(), "spawn");
        assert_eq!(Step::Post.dir(), "post");
        assert_eq!(Step::After.dir(), "after");
    }

    #[test]
    fn step_next() {
        assert_eq!(Step::Bite.next(), Some("spawn".to_string()));
        assert_eq!(Step::Spawn.next(), Some("post".to_string()));
        assert_eq!(Step::Post.next(), Some("after".to_string()));
        assert_eq!(Step::After.next(), None);
    }

    #[test]
    fn step_dir_from() {
        assert_eq!(Step::Bite.dir_from(), "");
        assert_eq!(Step::Spawn.dir_from(), "bite");
        assert_eq!(Step::Post.dir_from(), "spawn");
        assert_eq!(Step::After.dir_from(), "post");
    }

    // Test TOML parsing directly without file I/O
    #[test]
    fn zombie_bite_config_from_toml_string() {
        let toml_content = r#"
            base_path = "/custom/path"
            and_spawn = true
            with_monitor = false

            [relaychain]
            network = "kusama"
            runtime_override = "/path/to/runtime.wasm"

            [[parachains]]
            type = "asset-hub"
            enabled = true

            [[parachains]]
            type = "coretime"
            enabled = false
        "#;

        let config: ZombieBiteConfig = toml::from_str(toml_content).unwrap();

        assert_eq!(config.relaychain.network, "kusama");
        assert_eq!(
            config.relaychain.runtime_override,
            Some("/path/to/runtime.wasm".to_string())
        );
        assert_eq!(config.base_path, Some("/custom/path".to_string()));
        assert_eq!(config.and_spawn, Some(true));
        assert_eq!(config.with_monitor, Some(false));

        let parachains = config.get_parachains();
        assert_eq!(parachains.len(), 1); // Only asset-hub enabled
        assert_eq!(parachains[0].id(), 1000); // asset-hub ID
    }

    #[test]
    fn zombie_bite_config_minimal_toml() {
        let toml_content = r#"
[relaychain]
network = "polkadot"
        "#;

        let config: ZombieBiteConfig = toml::from_str(toml_content).unwrap();

        assert_eq!(config.relaychain.network, "polkadot");
        assert_eq!(config.relaychain.runtime_override, None);
        assert_eq!(config.parachains, None);
        assert_eq!(config.base_path, None);
        assert_eq!(config.and_spawn, None);
        assert_eq!(config.with_monitor, None);

        let parachains = config.get_parachains();
        assert_eq!(parachains.len(), 0); // No parachains specified
    }
}

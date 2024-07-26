//! Extracted from [substrate-minimal](https://github.com/hack-ink/substrate-minimal/blob/main/subspector/src/lib.rs)
//! Representation of chain-spec in `raw` mode

use fxhash::FxHashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

// TODO: doc
#[allow(missing_docs)]
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChainSpec {
	#[serde(default)]
	pub name: String,
	#[serde(default)]
	pub id: String,
	#[serde(default)]
	pub chain_type: String,
	#[serde(default)]
	pub boot_nodes: Vec<String>,
	pub telemetry_endpoints: Option<Value>,
	pub protocol_id: Option<String>,
	// TODO: for latest substrate version
	// #[serde(default = "Default::default", skip_serializing_if = "Option::is_none")]
	// pub fork_id: Option<String>,
	pub properties: Option<Value>,
	#[serde(default, flatten)]
	pub extensions: Value,
	#[serde(default)]
	pub consensus_engine: (),
	#[serde(default)]
	pub genesis: Genesis,
	#[serde(default)]
	pub code_substitutes: Value,
}
// TODO: doc
#[allow(missing_docs)]
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Genesis {
	#[serde(default)]
	pub raw: Raw,
}
// TODO: doc
#[allow(missing_docs)]
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Raw {
	#[serde(default)]
	pub top: FxHashMap<String, String>,
	#[serde(default)]
	pub children_default: FxHashMap<String, FxHashMap<String, String>>,
}


pub fn override_top(mut a: ChainSpec, b: ChainSpec) -> ChainSpec {
	let a_state = &mut a.genesis.raw.top;
	let b_state = b.genesis.raw.top;

	b_state.into_iter().for_each(|(k, v)| {
		a_state.insert(k, v);
	});

	a
}
//! Extracted from [subalfred - fork-off](https://github.com/hack-ink/subalfred/blob/main/lib/core/src/state/fork_off.rs)
//! Fork-off core library.

use std::path::PathBuf;
// std
use std::{mem, path::Path};

use anyhow::anyhow;
use fxhash::FxHashMap;
use tokio::try_join;
use crate::chain_spec_raw::{ChainSpec, override_top};
use crate::config::Context;
use crate::utils::{read_file_to_struct, write_data_to_file};

/// Fork-off configurations.
#[cfg_attr(feature = "clap", derive(Args))]
#[derive(Debug)]
pub struct ForkOffConfig {
	/// Renew the consensus relate things of the chain.
	///
	/// We need the dev chain specification to renew the consensus relates genesis. Otherwise, the
	/// fork-off chain won't produce block.
	///
	/// It will:
	/// - Skip `["System", "Babe", "Authorship", "Session", "Grandpa", "Beefy"]` pallets, but keep
	///   the `System::Account` data. (in order to make the new chain runnable)
	/// - Change the id and impl name to `*-export`.
	/// - Clear the bootnodes.
	/// - Set the `Staking::ForceEra` to `ForceNone`. (in order to prevent the validator set from
	///   changing mid-test)
	///
	/// Usually use this as below to get a runnable fork-off chain, and you can do whatever you
	/// want on it. Test new features, runtime upgrade, etc.
	///
	/// ```sh
	/// xxx-node export-state > xxx-export.json
	/// xxx-node build-spec --raw xxx-dev > xxx-dev.json
	/// subalfred state fork-off xxx-export.json --renew-consensus-with xxx-dev.json --simple-governance --disable-default-bootnodes
	/// xxx-node --chain xxx-export.json.fork-off --alice --tmp
	/// ```
	///
	/// Note:
	/// `--alice` only works for which dev chain's genesis validator is `//Alice`, otherwise the
	/// new chain won't produce block. If your dev chain's genesis validator is `//Bob`, then
	/// running with `--bob`. But if your dev chain's genesis validator isn't any one of the
	/// well-known keys, then you should start the node with `--validator` and insert the key
	/// manually.
	#[cfg_attr(
		feature = "clap",
		arg(verbatim_doc_comment, long, value_name = "PATH", conflicts_with = "all")
	)]
	pub renew_consensus_with: Option<String>,
	/// Use `//Alice` to control the governance.
	///
	/// It's useful when you want to test the runtime upgrade.
	///
	/// It will:
	/// - Replace sudo key with `//Alice`, if the sudo pallet existed.
	/// - Replace phragmen election and council members with `//Alice`, if the collective pallet
	///   existed.
	/// - Replace technical membership and tech.comm members with `//Alice`, if the membership
	///   pallet existed.
	#[cfg_attr(feature = "clap", arg(verbatim_doc_comment, long, conflicts_with = "all"))]
	pub simple_governance: bool,
	/// Disable adding the default bootnodes to the specification.
	#[cfg_attr(feature = "clap", arg(verbatim_doc_comment, long))]
	pub disable_default_bootnodes: bool,
}

/// Fork-off the state with the specific configurations.
pub async fn fork_off<P>(target_chain_spec_path: P, config: &ForkOffConfig, context: Context) -> Result<PathBuf, anyhow::Error>
where
	P: AsRef<Path>,
{
	let target_chain_spec_path = target_chain_spec_path.as_ref();
	let ForkOffConfig { renew_consensus_with, simple_governance, disable_default_bootnodes } =
		config;
	let mut chain_spec = if let Some(renew_consensus_with) = renew_consensus_with {
		let (mut target_chain_spec, dev_chain_spec) =
        try_join!(read_file_to_struct(target_chain_spec_path), read_file_to_struct(renew_consensus_with))?;

		match context {
    		Context::Relaychain => clear_consensus(&mut target_chain_spec),
    		Context::Parachain => clear_para_consensus(&mut target_chain_spec),
		}

		override_top(dev_chain_spec, target_chain_spec)
	} else {
		read_file_to_struct::<_, ChainSpec>(target_chain_spec_path).await?
	};

	if *simple_governance {
		set_simple_governance(&mut chain_spec);
	}
	if *disable_default_bootnodes {
		chain_spec.boot_nodes.clear();
	}

    let forked_path = PathBuf::try_from(format!("{}.{}", target_chain_spec_path.to_string_lossy(),"fork-off"))?;
    let data = &serde_json::to_vec(
            &serde_json::to_value(chain_spec).map_err(|_| anyhow!("Error generating a serde Value from chain-spec"))?
        ).map_err(|_| anyhow!("generic Serde serialization errror"))?;

    write_data_to_file(&forked_path, data).await?;
    Ok(forked_path)
}


fn clear_consensus(chain_spec: &mut ChainSpec) {
	let top = &mut chain_spec.genesis.raw.top;
	let system_prefix = array_bytes::bytes2hex("0x", subhasher::twox128(b"System"));
	let system_account_prefix =
		array_bytes::bytes2hex("0x", substorager::storage_value_key(&b"System"[..], b"Account"));
	// TODO: if the `top` is sorted, we can pop the prefix while it is passed
	let ignore_prefixes = [b"Babe".as_ref(), b"Authorship", b"Session", b"Grandpa", b"Beefy"]
		.iter()
		.map(|prefix| array_bytes::bytes2hex("0x", subhasher::twox128(prefix)))
		.collect::<Vec<_>>();
	// TODO: use `BTreeMap` for `top`, sortable
	let mut new_top = FxHashMap::default();

	mem::swap(top, &mut new_top);

	*top = new_top
		.into_iter()
		.filter_map(|(k, v)| {
			if k.starts_with(&system_prefix) {
				if k.starts_with(&system_account_prefix) {
					Some((k, v))
				} else {
					None
				}
			} else if ignore_prefixes.iter().any(|prefix| k.starts_with(prefix)) {
				None
			} else {
				Some((k, v))
			}
		})
		.collect();

	top.insert(
		substorager::storage_value_key(&b"Staking"[..], b"ForceEra").to_string(),
		"0x02".into(),
	);
	top.remove(&substorager::storage_value_key(&b"System"[..], b"LastRuntimeUpgrade").to_string());
}

fn clear_para_consensus(chain_spec: &mut ChainSpec) {
	let top = &mut chain_spec.genesis.raw.top;
	let system_prefix = array_bytes::bytes2hex("0x", subhasher::twox128(b"System"));
	let system_account_prefix =
		array_bytes::bytes2hex("0x", substorager::storage_value_key(&b"System"[..], b"Account"));
	// TODO: if the `top` is sorted, we can pop the prefix while it is passed
	let ignore_prefixes = [b"Aura".as_ref(), b"Authorship", b"Session"]
		.iter()
		.map(|prefix| array_bytes::bytes2hex("0x", subhasher::twox128(prefix)))
		.collect::<Vec<_>>();
	// TODO: use `BTreeMap` for `top`, sortable
	let mut new_top = FxHashMap::default();

	mem::swap(top, &mut new_top);

	*top = new_top
		.into_iter()
		.filter_map(|(k, v)| {
			if k.starts_with(&system_prefix) {
				if k.starts_with(&system_account_prefix) {
					Some((k, v))
				} else {
					None
				}
			} else if ignore_prefixes.iter().any(|prefix| k.starts_with(prefix)) {
				None
			} else {
				Some((k, v))
			}
		})
		.collect();

	// top.insert(
	// 	substorager::storage_value_key(&b"Staking"[..], b"ForceEra").to_string(),
	// 	"0x02".into(),
	// );
	top.remove(&substorager::storage_value_key(&b"System"[..], b"LastRuntimeUpgrade").to_string());
}

pub(super) fn set_simple_governance(chain_spec: &mut ChainSpec) {
	let top = &mut chain_spec.genesis.raw.top;
	let alice = "0xd43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d";
	let alice_members = "0x04d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d";
	// TODO: this might be different on different chain
	let alice_phragmen_election = "0x04d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d0010a5d4e800000000000000000000000010a5d4e80000000000000000000000";
	let council = substorager::storage_value_key(&b"Council"[..], b"Members");
	let technical_committee =
		substorager::storage_value_key(&b"TechnicalCommittee"[..], b"Members");
	let phragmen_election = substorager::storage_value_key(&b"PhragmenElection"[..], b"Members");
	let technical_membership =
		substorager::storage_value_key(&b"TechnicalMembership"[..], b"Members");
	let sudo = substorager::storage_value_key(&b"Sudo"[..], b"Key");

	// TODO: skip if not exist
	top.insert(council.to_string(), alice_members.into());
	top.insert(technical_committee.to_string(), alice_members.into());
	top.insert(technical_membership.to_string(), alice_members.into());
	top.insert(phragmen_election.to_string(), alice_phragmen_election.into());
	top.insert(sudo.to_string(), alice.into());
}

use codec::{CompactAs, Decode, Encode, MaxEncodedLen};
/// Parachain id.
///
/// This is an equivalent of the `polkadot_parachain_primitives::Id`, which is a compact-encoded
/// `u32`.
#[derive(
	Clone,
	CompactAs,
	Copy,
	Decode,
	Default,
	Encode,
	Eq,
	Hash,
	MaxEncodedLen,
	Ord,
	PartialEq,
	PartialOrd,
)]
pub struct ParaId(pub u32);

impl From<u32> for ParaId {
	fn from(id: u32) -> Self {
		ParaId(id)
	}
}

#[cfg(test)]
mod test {
    use codec::Encode;
    use super::ParaId;

    #[test]
    fn heads(){
        let paras_head_prefix = "0xcd710b30bd2eab0352ddcc26417aa1941b3c252fcb29d88eff4f3de5de4476c3";
        let paras_head_prefix_gen = array_bytes::bytes2hex("0x", substorager::storage_value_key(&b"Paras"[..], b"Heads"));
        assert_eq!(paras_head_prefix, paras_head_prefix_gen.as_str());

        let inv = "0x15464cac3378d46f113cd5b7a4d71c845579297f4dfb9609e7e4c2ebab9ce40a";
		let inv_gen = array_bytes::bytes2hex("0x", substorager::storage_value_key(&b"CollatorSelection"[..], b"Invulnerables"));
        assert_eq!(inv, inv_gen.as_str());

        // let inv_3 = array_bytes::bytes2hex("0x", substorager::storage_value_key(&b"System"[..], b"Account"));
        // let inv = "0x15464cac3378d46f113cd5b7a4d71c845579297f4dfb9609e7e4c2ebab9ce40a";
        // assert_eq!(inv, inv_3.as_str());

        // let inv_3 = array_bytes::bytes2hex("0x", substorager::storage_value_key(&b"Paras"[..], b"CodeByHash"));
        // let inv = "0x15464cac3378d46f113cd5b7a4d71c845579297f4dfb9609e7e4c2ebab9ce40a";
        // assert_eq!(inv, inv_3.as_str());
		let w = include_bytes!("/tmp/z/asset-hub.wasm");
		let r = include_str!("/tmp/z/asset-hub.wasm");
		let w_decoded = hex::decode(&w[2..]).unwrap();
		let r_decoded = hex::decode(&r[2..]).unwrap();
		assert_eq!(w_decoded, r_decoded);

		let w_hash = subhasher::blake2_256(&w_decoded[..]);
		println!("{:?}", array_bytes::bytes2hex("",w_hash));

        let inv_3 = array_bytes::bytes2hex("0x", substorager::storage_value_key(&b"Paras"[..], b"CodeByHash"));
        let inv = "0x15464cac3378d46f113cd5b7a4d71c845579297f4dfb9609e7e4c2ebab9ce40a";
        assert_eq!(inv, inv_3.as_str());

    }

	#[test]
	fn demo() {
		let prefix = array_bytes::bytes2hex("0x", subhasher::twox128(b"CollatorSelection"));

		let inv = array_bytes::bytes2hex("0x", substorager::storage_value_key(&b"CollatorSelection"[..], b"Invulnerables"));
		println!("aura {}",array_bytes::bytes2hex("0x", subhasher::twox128(b"Aura")));
		println!("babe {}",array_bytes::bytes2hex("0x", subhasher::twox128(b"Babe")));
		println!("session {}",array_bytes::bytes2hex("0x", subhasher::twox128(b"Session")));
		println!("grandpa {}",array_bytes::bytes2hex("0x", subhasher::twox128(b"Grandpa")));
		println!("Authorship {}",array_bytes::bytes2hex("0x", subhasher::twox128(b"Authorship")));
		println!("Beefy {}",array_bytes::bytes2hex("0x", subhasher::twox128(b"Beefy")));
		println!("sys_acc {}", array_bytes::bytes2hex("0x", substorager::storage_value_key(&b"System"[..], b"Account")));

		println!("prefix: {prefix}");
		println!("invulnerables: {inv}");
	}

    #[test]
    fn para_id() {
        let para_id:ParaId = 1000_u32.into();
        let encoded = para_id.encode();
        // assert_eq!(array_bytes::bytes2hex("", &encoded), "");
        let hash = subhasher::twox64_concat(&encoded);
        assert_eq!(array_bytes::bytes2hex("", &hash), "");
    }
}
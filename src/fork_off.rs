//! Extracted from [subalfred - fork-off](https://github.com/hack-ink/subalfred/blob/main/lib/core/src/state/fork_off.rs)
//! Fork-off core library.

use std::collections::HashMap;
use std::path::PathBuf;
// std
use std::{mem, path::Path};

use crate::chain_spec_raw::{override_top, ChainSpec};
use crate::config::Context;
use crate::utils::{read_file_to_struct, write_data_to_file};
use anyhow::anyhow;
use fxhash::FxHashMap;
use tokio::try_join;

pub type ParasHeads = HashMap<u32, String>;

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
    pub renew_consensus_with: String,
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
    pub simple_governance: bool,
    /// Disable adding the default bootnodes to the specification.
    pub disable_default_bootnodes: bool,
    // HashMap of ParaId: Head (only used when fork-off relaychain)
    pub paras_heads: ParasHeads,
}

/// Fork-off the state with the specific configurations.
pub async fn fork_off<P>(
    target_chain_spec_path: P,
    config: &ForkOffConfig,
    context: Context,
) -> Result<PathBuf, anyhow::Error>
where
    P: AsRef<Path>,
{
    let target_chain_spec_path = target_chain_spec_path.as_ref();
    let ForkOffConfig {
        renew_consensus_with,
        simple_governance,
        disable_default_bootnodes,
        paras_heads,
    } = config;
    let (mut target_chain_spec, dev_chain_spec) = try_join!(
        read_file_to_struct(target_chain_spec_path),
        read_file_to_struct(renew_consensus_with)
    )?;

    match context {
        Context::Relaychain => clear_consensus(&mut target_chain_spec, paras_heads.to_owned()),
        Context::Parachain => clear_para_consensus(&mut target_chain_spec),
    }

    let mut chain_spec = override_top(dev_chain_spec, target_chain_spec);

    if *simple_governance {
        set_simple_governance(&mut chain_spec);
    }
    if *disable_default_bootnodes {
        chain_spec.boot_nodes.clear();
    }

    let forked_path = PathBuf::try_from(format!(
        "{}.{}",
        target_chain_spec_path.to_string_lossy(),
        "fork-off"
    ))?;
    let data = &serde_json::to_vec_pretty(
        &serde_json::to_value(&chain_spec)
            .map_err(|_| anyhow!("Error generating a serde Value from chain-spec"))?,
    )
    .map_err(|_| anyhow!("generic Serde serialization errror"))?;

    write_data_to_file(&forked_path, data).await?;
    Ok(forked_path)
}

fn clear_consensus(chain_spec: &mut ChainSpec, paras_heads: ParasHeads) {
    let top = &mut chain_spec.genesis.raw.top;
    let system_prefix = array_bytes::bytes2hex("0x", subhasher::twox128(b"System"));
    let system_account_prefix = array_bytes::bytes2hex(
        "0x",
        substorager::storage_value_key(&b"System"[..], b"Account"),
    );
    // TODO: if the `top` is sorted, we can pop the prefix while it is passed
    let ignore_prefixes = [
        b"Babe".as_ref(),
        b"Authorship",
        b"Session",
        b"Grandpa",
        b"Beefy",
    ]
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

    // override heads
    let paras_head_prefix = array_bytes::bytes2hex(
        "0x",
        substorager::storage_value_key(&b"Paras"[..], b"Heads"),
    );
    for (id, head) in paras_heads.into_iter() {
        // construct the key
        let para_id: ParaId = id.into();
        let encoded = para_id.encode();
        let para_id_hash = subhasher::twox64_concat(&encoded);
        let key = format!(
            "{paras_head_prefix}{}",
            array_bytes::bytes2hex("", &para_id_hash)
        );
        let para_head =
            array_bytes::bytes2hex("0x", HeadData(hex::decode(&head[2..]).unwrap()).encode());
        println!("key: {key}");
        println!("key: {para_head}");

        top.insert(key, para_head);
    }

    top.insert(
        substorager::storage_value_key(&b"Staking"[..], b"ForceEra").to_string(),
        "0x02".into(),
    );
    top.remove(&substorager::storage_value_key(&b"System"[..], b"LastRuntimeUpgrade").to_string());
    top.remove(
        &substorager::storage_value_key(&b"ParaScheduler"[..], b"SessionStartBlock").to_string(),
    );
}

fn clear_para_consensus(chain_spec: &mut ChainSpec) {
    let top = &mut chain_spec.genesis.raw.top;
    let system_prefix = array_bytes::bytes2hex("0x", subhasher::twox128(b"System"));
    let system_account_prefix = array_bytes::bytes2hex(
        "0x",
        substorager::storage_value_key(&b"System"[..], b"Account"),
    );
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
    top.insert(
        phragmen_election.to_string(),
        alice_phragmen_election.into(),
    );
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
pub struct Bl(pub u32);

/// Parachain head data included in the chain.
#[derive(
    PartialEq,
    Eq,
    Clone,
    PartialOrd,
    Ord,
    Encode,
    Decode,
    // RuntimeDebug,
    // derive_more::From,
    // TypeInfo,
    // Serialize,
    // Deserialize,
)]
pub struct HeadData(pub Vec<u8>);

#[cfg(test)]
mod test {
    use super::*;

    use super::ParaId;
    use crate::{config, fork_off::ForkOffConfig};
    use codec::Encode;

    #[test]
    fn heads() {
        let paras_head_prefix =
            "0xcd710b30bd2eab0352ddcc26417aa1941b3c252fcb29d88eff4f3de5de4476c3";
        let paras_head_prefix_gen = array_bytes::bytes2hex(
            "0x",
            substorager::storage_value_key(&b"Paras"[..], b"Heads"),
        );
        assert_eq!(paras_head_prefix, paras_head_prefix_gen.as_str());

        // let paras_head_prefix = "0xcd710b30bd2eab0352ddcc26417aa1941b3c252fcb29d88eff4f3de5de4476c3";
        // let paras_head_prefix_gen = array_bytes::bytes2hex("0x", substorager::storage_value_key(&b"Paras"[..], b"MostRecentContext"));
        // assert_eq!(paras_head_prefix, paras_head_prefix_gen.as_str());

        // let inv = "0x15464cac3378d46f113cd5b7a4d71c845579297f4dfb9609e7e4c2ebab9ce40a";
        // let inv_gen = array_bytes::bytes2hex("0x", substorager::storage_value_key(&b"CollatorSelection"[..], b"Invulnerables"));
        // assert_eq!(inv, inv_gen.as_str());

        // let inv_3 = array_bytes::bytes2hex("0x", substorager::storage_value_key(&b"Paras"[..], b"CodeByHash"));
        // let inv = "0x15464cac3378d46f113cd5b7a4d71c845579297f4dfb9609e7e4c2ebab9ce40a";
        // assert_eq!(inv, inv_3.as_str());
    }

    #[test]
    fn demo() {
        let prefix = array_bytes::bytes2hex("0x", subhasher::twox128(b"CollatorSelection"));

        let inv = array_bytes::bytes2hex(
            "0x",
            substorager::storage_value_key(&b"CollatorSelection"[..], b"Invulnerables"),
        );
        println!(
            "aura authorities{}",
            array_bytes::bytes2hex(
                "0x",
                substorager::storage_value_key(&b"Aura"[..], b"Authorities")
            )
        );

        println!(
            "aura {}",
            array_bytes::bytes2hex("0x", subhasher::twox128(b"Aura"))
        );
        println!(
            "babe {}",
            array_bytes::bytes2hex("0x", subhasher::twox128(b"Babe"))
        );
        println!(
            "babe_authorities {}",
            array_bytes::bytes2hex(
                "0x",
                substorager::storage_value_key(&b"Babe"[..], b"Authorities")
            )
        );
        println!(
            "babe NextAuthorities {}",
            array_bytes::bytes2hex(
                "0x",
                substorager::storage_value_key(&b"Babe"[..], b"NextAuthorities")
            )
        );
        println!(
            "session {}",
            array_bytes::bytes2hex("0x", subhasher::twox128(b"Session"))
        );
        println!(
            "grandpa {}",
            array_bytes::bytes2hex("0x", subhasher::twox128(b"Grandpa"))
        );
        println!(
            "grandpa Authorities {}",
            array_bytes::bytes2hex(
                "0x",
                substorager::storage_value_key(&b"Grandpa"[..], b"Authorities")
            )
        );

        println!(
            "Authorship {}",
            array_bytes::bytes2hex("0x", subhasher::twox128(b"Authorship"))
        );
        println!(
            "Beefy {}",
            array_bytes::bytes2hex("0x", subhasher::twox128(b"Beefy"))
        );
        println!(
            "sys_acc {}",
            array_bytes::bytes2hex(
                "0x",
                substorager::storage_value_key(&b"System"[..], b"Account")
            )
        );
        println!(
            "ParaScheduler_SessionStartBlock {}",
            array_bytes::bytes2hex(
                "0x",
                substorager::storage_value_key(&b"ParaScheduler"[..], b"SessionStartBlock")
            )
        );

        println!("prefix: {prefix}");
        println!("invulnerables: {inv}");
    }

    #[test]
    fn para_id() {
        let para_id: ParaId = 1005_u32.into();
        let encoded = para_id.encode();
        let hash = subhasher::twox64_concat(&encoded);
        assert_eq!(
            array_bytes::bytes2hex("", &hash),
            "d6dbddd5e1a9eb49ed030000"
        );
    }

    #[test]
    fn block() {
        // let b = hex::decode("4c3e7201").unwrap();
        // let z: u32 = Vec<u8>::decode(b);// b.decode();
        let z = Bl(24264268).encode();
        println!("{}", array_bytes::bytes2hex("0x", &z));

        let z = Bl(0).encode();
        println!("{}", array_bytes::bytes2hex("0x", &z));
        let c = hex::decode("000000000000000000000000000000000000000000000000000000000000000000634283f891c6ecfa542be496ad576bd40167219cdb0fc8a81b071f4e312d9ac503170a2e7597b7b7e3d84c05391d139a62b157e78786d8c082f29dcf4c11131400").unwrap();
        let h = HeadData(c).encode();
        println!("{}", array_bytes::bytes2hex("0x", &h));
    }

    #[test]
    fn vec_to_str() {
        let v = [
            48, 120, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48,
            48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48,
            48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48, 48,
            48, 48, 48, 49, 48, 52, 56, 56, 51, 102, 100, 51, 97, 48, 53, 50, 48, 51, 100, 48, 52,
            49, 100, 97, 50, 97, 50, 54, 56, 52, 56, 55, 98, 49, 51, 52, 49, 50, 102, 53, 48, 102,
            56, 56, 53, 57, 97, 57, 100, 97, 102, 52, 97, 101, 56, 55, 55, 54, 50, 54, 97, 54, 57,
            100, 101, 97, 100, 48, 51, 49, 55, 48, 97, 50, 101, 55, 53, 57, 55, 98, 55, 98, 55,
            101, 51, 100, 56, 52, 99, 48, 53, 51, 57, 49, 100, 49, 51, 57, 97, 54, 50, 98, 49, 53,
            55, 101, 55, 56, 55, 56, 54, 100, 56, 99, 48, 56, 50, 102, 50, 57, 100, 99, 102, 52,
            99, 49, 49, 49, 51, 49, 52, 48, 48,
        ];
        let s = String::from_utf8_lossy(&v);
        println!("{s}");
    }

    #[ignore = "test file needed"]
    #[tokio::test]
    async fn create() {
        let fork_off_config = ForkOffConfig {
            renew_consensus_with: "/tmp/z/kusama-local-2.json".to_string(),
            simple_governance: false,
            disable_default_bootnodes: true,
            paras_heads: Default::default(),
        };
        let exported_state_file = String::from("/tmp/z/sync-db/exported-state.json");
        println!("{}", &exported_state_file);
        println!("{:?}", fork_off_config);

        let forked_off_path = fork_off(
            &exported_state_file,
            &fork_off_config,
            config::Context::Relaychain,
        )
        .await
        .unwrap();
        println!("{:?}", forked_off_path);
    }
}

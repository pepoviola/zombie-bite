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
// #[cfg_attr(feature = "clap", derive(Args))]
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
        debug!("key: {key}");
        debug!("value: {para_head}");

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
use tracing::debug;
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
pub struct CoreIndex(pub u32);
impl From<u32> for CoreIndex {
    fn from(id: u32) -> Self {
        CoreIndex(id)
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
#[derive(PartialEq, Eq, Clone, PartialOrd, Ord, Encode, Decode)]
pub struct HeadData(pub Vec<u8>);

/// Parachain head data included in the chain.
#[derive(PartialEq, Eq, Clone, PartialOrd, Ord, Encode, Decode)]
pub struct MessageQueueChain(pub sp_core::H256);

#[cfg(test)]
mod test {
    use super::*;

    use super::ParaId;
    use crate::{config, fork_off::ForkOffConfig};
    use codec::Encode;

    #[test]
    fn encode_u32() {
        let one = 1_u32;
        let encoded = one.encode();
        println!("{}", array_bytes::bytes2hex("0x", encoded));
    }

    #[test]
    fn twox256_works() {
        let idx: CoreIndex = 0.into();
        let zero = subhasher::twox256(idx.encode());
        println!("{}", array_bytes::bytes2hex("0x", zero));
        let val = MessageQueueChain(sp_core::H256::zero()).encode();
        println!("{}", array_bytes::bytes2hex("0x", val));
    }

    #[test]
    fn heads() {
        let paras_head_prefix =
            "0xcd710b30bd2eab0352ddcc26417aa1941b3c252fcb29d88eff4f3de5de4476c3";
        let paras_head_prefix_gen = array_bytes::bytes2hex(
            "0x",
            substorager::storage_value_key(&b"Paras"[..], b"Heads"),
        );
        assert_eq!(paras_head_prefix, paras_head_prefix_gen.as_str());

        // construct the key
        let para_id: ParaId = 1000_u32.into();
        let encoded = para_id.encode();
        let para_id_hash = subhasher::twox64_concat(&encoded);
        println!("paraId(1000) {}", array_bytes::bytes2hex("", &para_id_hash));
        let key = format!(
            "{paras_head_prefix}{}",
            array_bytes::bytes2hex("", &para_id_hash)
        );

        let head = "0x8a98384334fa4699a25a20227322f240d440bb2c80342cac1c3ba82999963cb15240c90177a61cae36ce0de8652e5e6d0a9a3e73d4fbeb736fc1c4e4b0c9b86e90c50eaf846fc952f36dc6a8f28f25e187fc7347c942c0960ce5acd2b0d4421cebdc6d790c0661757261204ed49808000000000452505352909cd3b9bdee77156c8e5c74e83dbbf7faaf5d66a98bf8dc630e56cf9c628157a27ebe8c05056175726101019ecb399a86c3536ff8e7ea65890d83655126c1e9673dbf31059b7d37a37e5c3dd70d329102ba7b876c0b463547ba021a7c4aefe706b441012a26dc1b950f0c00";

        // let head = "0xa8b352f2abae4d6761e27903d21c0b0ef82b19f88a669fb2d8e0dac8f6cec0f61addc801ba0cf1c2c5d8b9a9a6e61b812acb484d1a39180c5c0e1441150e93917b6d8474765ccf155e57e445bd443b8dcdd09400cdeb6d2d5a728106f87ebd3cb38b076d0c06617572612064bb980800000000045250535290bcff4cafc894eec61315567012c193803e1f49d990b04e95cc570e5a31c8a9cf4ef78b050561757261010134c68400fe59c0c9e5cfdc6b6868f654055b3c2046a3486945b1c8e3176411f4332dadd7bac02296420b036b77ab921f8aba089052c80a20e54f02a19d1a2203";
        // "0x432a9ed9f85634d93ae6a05d0a451fe3e23b78c06d5bb30829d36067dc2127ea223bc9013f1250bd5b953710939adf755ce591a825c30ae04017e7f62a14cbac0b3b69cd563957aef72d5220cd5f2bd5c0efccadca9a47342136d68991c42052f6ff0f7c0c066175726120f7d29808000000000452505352905246b49bce4cd6943e60d1f886182f755733a6c25006d3cff34b7f50d66cf157d2b38c05056175726101013f39cf0bdc7cf05c11801829c0fb253a3493c2cc90bb9eb1858e7bf0a047e39f8dd89f6a8221599a94c31a2a2820d50c09ecd36a048dec83515906e85274dd0e";
        let para_head =
            array_bytes::bytes2hex("0x", HeadData(hex::decode(&head[2..]).unwrap()).encode());
        println!("asset-hub(1000) : {}", key);
        println!("asset-hub(1000) head: {}", para_head);

        let vec_of_paras: Vec<ParaId> = vec![];
        println!(
            "vec od paras : {}",
            array_bytes::bytes2hex("0x", vec_of_paras.encode())
        );
    }

    #[test]
    fn demo() {
        let prefix = array_bytes::bytes2hex("0x", subhasher::twox128(b"CollatorSelection"));

        let gap = array_bytes::bytes2hex("0x", subhasher::twox128(b"gap"));
        println!("gap: {gap}");

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
            "grandpa voters {}",
            array_bytes::bytes2hex(
                "0x",
                substorager::storage_value_key(&b"Grandpa"[..], b"voters")
            )
        );

        println!(
            "grandpa CurrentSetId {}",
            array_bytes::bytes2hex(
                "0x",
                substorager::storage_value_key(&b"Grandpa"[..], b"CurrentSetId")
            )
        );

        println!(
            "grandpa Stalled {}",
            array_bytes::bytes2hex(
                "0x",
                substorager::storage_value_key(&b"Grandpa"[..], b"Stalled")
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

        println!(
            "Staking_Invulnerables {}",
            array_bytes::bytes2hex(
                "0x",
                substorager::storage_value_key(&b"Staking"[..], b"Invulnerables")
            )
        );

        println!(
            "Staking_Nominators {}",
            array_bytes::bytes2hex(
                "0x",
                substorager::storage_value_key(&b"Staking"[..], b"Nominators")
            )
        );

        println!(
            "Staking_MinimumValidatorCount {}",
            array_bytes::bytes2hex(
                "0x",
                substorager::storage_value_key(&b"Staking"[..], b"MinimumValidatorCount")
            )
        );

        println!(
            "Configuration_ActiveConfig {}",
            array_bytes::bytes2hex(
                "0x",
                substorager::storage_value_key(&b"Configuration"[..], b"ActiveConfig")
            )
        );

        println!(
            "Paras_CurrentCodeHash {}",
            array_bytes::bytes2hex(
                "0x",
                substorager::storage_value_key(&b"Paras"[..], b"CurrentCodeHash")
            )
        );

        println!(
            "Sudo_Key {}",
            array_bytes::bytes2hex("0x", substorager::storage_value_key(&b"Sudo"[..], b"Key"))
        );
        println!("prefix: {prefix}");
        println!("invulnerables: {inv}");
    }

    #[test]
    fn para_id() {
        let para_id: ParaId = 1000_u32.into();
        let encoded = para_id.encode();
        println!("encoded paraId: {}", array_bytes::bytes2hex("", &encoded));
        let hash = subhasher::twox64_concat(&encoded);
        assert_eq!(
            array_bytes::bytes2hex("", &hash),
            "d6dbddd5e1a9eb49ed030000"
        );
    }

    #[test]
    pub fn test_to_hex() {
        println!("grandpa_voters");
        assert_eq!(hex::encode(b"grandpa_voters"), "666f6f626172");
    }

    #[test]
    fn block() {
        let z = Bl(24264268).encode();
        println!("{}", array_bytes::bytes2hex("0x", &z));

        let z = Bl(0).encode();
        println!("{}", array_bytes::bytes2hex("0x", &z));
        let c = hex::decode("000000000000000000000000000000000000000000000000000000000000000000634283f891c6ecfa542be496ad576bd40167219cdb0fc8a81b071f4e312d9ac503170a2e7597b7b7e3d84c05391d139a62b157e78786d8c082f29dcf4c11131400").unwrap();
        let h = HeadData(c).encode();
        println!("{}", array_bytes::bytes2hex("0x", &h));
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

    #[test]
    fn hex_test() {
        let val = "0xcd710b30bd2eab0352ddcc26417aa1941b3c252fcb29d88eff4f3de5de4476c3b6ff6f7d467b87a9e8030000";
        let a = hex::decode("cd710b30bd2eab0352ddcc26417aa1941b3c252fcb29d88eff4f3de5de4476c3b6ff6f7d467b87a9e8030000").unwrap();
        let b = hex::decode(&val[2..]).unwrap();
        assert_eq!(a, b);
    }
}

// paras.parachains
// 0xcd710b30bd2eab0352ddcc26417aa1940b76934f4cc08dee01012d059e1b83ee
// 0x04e8030000 (only 1000)

// paras.heads
// 0xcd710b30bd2eab0352ddcc26417aa1941b3c252fcb29d88eff4f3de5de4476c3

// paras.heads.1000
// 0xcd710b30bd2eab0352ddcc26417aa1941b3c252fcb29d88eff4f3de5de4476c3b6ff6f7d467b87a9e8030000

// paraScheduler.validatorGroup
// 0x94eadf0156a8ad5156507773d0471e4a16973e1142f5bd30d9464076794007db
// 0x041000000000010000000200000003000000 (one group of 4 valudators)

// paraScheduler.claimQueue
// 0x94eadf0156a8ad5156507773d0471e4a49f6c9aa90c04982c05388649310f22f
// 0x040000000000 (empty, will auto-fill?)

// paraShared.activeValidatorIndices
// 0xb341e3a63e58a188839b242d17f8c9f82586833f834350b4d435d5fd269ecc8b
// 0x1000000000030000000200000001000000 ( 4 validator shuffle)

// paraShared.activeValidatorKeys
// 0xb341e3a63e58a188839b242d17f8c9f87a50c904b368210021127f9238883a6e
// 0x10d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d306721211d5404bd9da88e0204360a1a9ab8b87c66c1bc2fcdd37f3c2222cc2090b5ab205c6974c9ea841be688864633dc9ca8a357843eeacf2314649965fe228eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a48

// authorityDiscovery.keys
// 0x2099d7f109d6e535fb000bba623fd4409f99a2ce711f3a31b2fc05604c93f179
// 0x10d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d8eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a4890b5ab205c6974c9ea841be688864633dc9ca8a357843eeacf2314649965fe22306721211d5404bd9da88e0204360a1a9ab8b87c66c1bc2fcdd37f3c2222cc20

// authorityDiscovery.nextKeys
// 0x2099d7f109d6e535fb000bba623fd4404c014e6bf8b8c2c011e7290b85696bb3
// 0x10d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d8eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a4890b5ab205c6974c9ea841be688864633dc9ca8a357843eeacf2314649965fe22306721211d5404bd9da88e0204360a1a9ab8b87c66c1bc2fcdd37f3c2222cc20

use crate::config::{Parachain, Relaychain};
use crate::utils::{get_validator_keys, ValidationCode};
use codec::Encode;
use serde_json::{json, Value};
use std::{env, path::PathBuf};
use tokio::fs;

pub async fn generate_default_overrides_for_rc(
    base_dir: &str,
    relay: &Relaychain,
    paras: &Vec<Parachain>,
) -> PathBuf {
    // Calculate required validators: 2 base + 1 per parachain (max 7)
    let num_validators = (2 + paras.len()).min(7);
    let validator_keys = get_validator_keys(num_validators);

    // Build stash list for validators (concatenated hex)
    let stash_list: String = validator_keys
        .iter()
        .map(|v| v.stash)
        .collect::<Vec<_>>()
        .join("");

    // Build session keys for NextKeys (inject)
    // Session.NextKeys uses TwoX64Concat hasher, so key format is: prefix + twox64(stash) + stash
    let mut next_keys_injects = json!({});
    for keys in &validator_keys {
        let stash_bytes = hex::decode(keys.stash).expect("stash should be valid hex");
        let stash_hash = array_bytes::bytes2hex("", subhasher::twox64(&stash_bytes));
        let inject_key = format!(
            "cec5070d609dd3497f72bde07fc96ba04c014e6bf8b8c2c011e7290b85696bb3{}{}",
            stash_hash, keys.stash
        );
        next_keys_injects[inject_key] = json!(keys.session_keys_encoded());
    }

    // Build QueuedKeys (stash + session_keys for each validator)
    let queued_keys: String = validator_keys
        .iter()
        .map(|v| v.session_keys_queuedkeys_format())
        .collect::<Vec<_>>()
        .join("");

    // Build Babe Authorities (babe_key + weight for each)
    let babe_authorities: String = validator_keys
        .iter()
        .map(|v| format!("{}0100000000000000", v.babe))
        .collect::<Vec<_>>()
        .join("");

    // Build Grandpa Authorities (grandpa_key + weight for each)
    let grandpa_authorities: String = validator_keys
        .iter()
        .map(|v| format!("{}0100000000000000", v.grandpa))
        .collect::<Vec<_>>()
        .join("");

    // Build authority discovery keys
    let authority_discovery_keys: String = validator_keys
        .iter()
        .map(|v| v.authority_discovery)
        .collect::<Vec<_>>()
        .join("");

    // Build validator indices for parachain shared
    let validator_indices: String = (0..num_validators)
        .map(|i| format!("{:02x}000000", i))
        .collect::<Vec<_>>()
        .join("");

    // Build para validator keys (same as authority discovery for our purposes)
    let para_validator_keys: String = validator_keys
        .iter()
        .map(|v| v.para_validator)
        .collect::<Vec<_>>()
        .join("");

    // Format validator count as compact encoded
    let validator_count_hex = format!("{:02x}", num_validators * 4); // *4 because we encode each as 4 bytes

    // Keys to inject (mostly storage maps that are not present in the current state)
    // <Pallet> < Item>
    let mut injects = next_keys_injects;

    // RcMigrator Manager (set //Alice by default)
    injects["2185d18cb42ae97242af0e70e6ad689012fcd13ee43ae32cc87f798eb5ed3295"] =
        json!("d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d");

    // Get the first parachain ID for parachain-specific overrides
    let first_para_id = paras.first().map(|p| p.id()).unwrap_or(1000);
    let para_id_hex = format!("{:08x}", first_para_id.to_le());

    // Build core descriptor for the first parachain
    let core_descriptor = format!("00010402{}00e100e100010000e1", para_id_hex);

    // Build DMP and HRMP storage keys with the parachain ID
    let dmp_queue_key = format!(
        "63f78c98723ddc9073523ef3beefda0c4d7fefc408aac59dbfe80a72ac8e3ce5b6ff6f7d467b87a9{}",
        para_id_hex
    );
    let hrmp_channels_key = format!(
        "6a0da05ca59913bc38a8630590f2627c1d3719f5b0b12c7105c073c507445948b6ff6f7d467b87a9{}",
        para_id_hex
    );

    // Build paras parachains list (just the first para)
    let paras_parachains = format!("04{}", para_id_hex);

    // <Pallet> <Item>
    // e.g Validator Validators

    let mut overrides = json!({
        // Validator Validators (dynamic list)
        "7d9fe37370ac390779f35763d98106e888dcde934c658227ee1dfafcd6e16903": format!("{}{}", validator_count_hex, stash_list),
        // Session Validators (dynamic list)
        "cec5070d609dd3497f72bde07fc96ba088dcde934c658227ee1dfafcd6e16903": format!("{}{}", validator_count_hex, stash_list),
        //  Session QueuedKeys (dynamic list)
        "cec5070d609dd3497f72bde07fc96ba0e0cdd062e6eaf24295ad4ccfc41d4609": format!("{}{}", validator_count_hex, queued_keys),
        // Babe Authorities (dynamic list)
        "1cb6f36e027abb2091cfb5110ab5087f5e0621c4869aa60c02be9adcc98a0d1d": format!("{}{}", validator_count_hex, babe_authorities),
        // Babe NextAuthorities (dynamic list)
        "1cb6f36e027abb2091cfb5110ab5087faacf00b9b41fda7a9268821c2a2b3e4c": format!("{}{}", validator_count_hex, babe_authorities),
        // Babe PendingEpochConfigChange
        "1cb6f36e027abb2091cfb5110ab5087f66e8f035c8adbe7f1547b43c51e6f8a4": "00",
        // Grandpa Authorities (dynamic list)
        "5f9cc45b7a00c5899361e1c6099678dc5e0621c4869aa60c02be9adcc98a0d1d": format!("{}{}", validator_count_hex, grandpa_authorities),
        // Staking ForceEra (ForceNone)
        "5f3e4907f716ac89b6347d15ececedcaf7dad0317324aecae8744b87fc95f2f3": "02",
        // Staking Invulnerables (dynamic list)
        "5f3e4907f716ac89b6347d15ececedca5579297f4dfb9609e7e4c2ebab9ce40a": format!("{}{}", validator_count_hex, stash_list),
        // paras parachains (dynamic based on first parachain)
        "cd710b30bd2eab0352ddcc26417aa1940b76934f4cc08dee01012d059e1b83ee": paras_parachains,
        // paraScheduler validatorGroup (dynamic groups based on validator count)
        "94eadf0156a8ad5156507773d0471e4a16973e1142f5bd30d9464076794007db": format!("{}{}", validator_count_hex, validator_indices),
        // paraScheduler claimQueue (empty, will auto-fill)
        "94eadf0156a8ad5156507773d0471e4a49f6c9aa90c04982c05388649310f22f": "040000000000",
        // paraShared activeValidatorIndices (dynamic)
        "b341e3a63e58a188839b242d17f8c9f82586833f834350b4d435d5fd269ecc8b": format!("{}{}", validator_count_hex, validator_indices),
        // paraShared activeValidatorKeys (dynamic)
        "b341e3a63e58a188839b242d17f8c9f87a50c904b368210021127f9238883a6e": format!("{}{}", validator_count_hex, para_validator_keys),
        // authorityDiscovery keys (dynamic)
        "2099d7f109d6e535fb000bba623fd4409f99a2ce711f3a31b2fc05604c93f179": format!("{}{}", validator_count_hex, authority_discovery_keys),
        // authorityDiscovery nextKeys (dynamic)
        "2099d7f109d6e535fb000bba623fd4404c014e6bf8b8c2c011e7290b85696bb3": format!("{}{}", validator_count_hex, authority_discovery_keys),
        // Core descriptor, ensure core 0 is assigned to first parachain (dynamic)
        "638595eebaa445ce03a13547bece90e704e6ac775a3245623103ffec2cb2c92fb4def25cfda6ef3ac02a707a7013b12ddc9c5f6a3e1994c51754be175bd6a3d4": core_descriptor,
        // Configuration activeConfig
        "06de3d8a54d27e44a9d5ce189618f22db4b49d95320d9021994c850f25b8e385": "0000300000500000aaaa020000001000fbff0000100000000a000000403800005802000003000000020000000000500000c800008000000000e8764817000000000000000000000000e87648170000000000000000000000e80300000090010080000000009001000c01002000000600c4090000000000000601983a00000000000040380000000600000058020000030000001900000000000000020000000200000002000000140000000100000008030100000014000000040000000105000000010000000100000000000000f401000080b2e60e80c3c90180b2e60e00000000000000000000000005000000",
        // paraScheduler availabilityCores (1 core, free)
        "94eadf0156a8ad5156507773d0471e4ab8ebad86f546c7e0b135a4212aace339": "0400",
        // Sudo Key (Alice)
        "5c0d1176a568c1f92944340dbfed9e9c530ebca703c85910e7164cb7d1c9e47b": "d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d"
    });

    // Add DMP and HRMP overrides dynamically (empty queues for the parachain)
    overrides[&dmp_queue_key] =
        json!("0000000000000000000000000000000000000000000000000000000000000000");
    overrides[&hrmp_channels_key] = json!("00");

    // update the overrides / injects map to use IFF the key is provided
    if let Ok(sudo_key) = env::var("ZOMBIE_SUDO") {
        // Sudo Key
        overrides["5c0d1176a568c1f92944340dbfed9e9c530ebca703c85910e7164cb7d1c9e47b"] =
            Value::String(sudo_key.clone());

        // RcMigrator Manager
        injects["2185d18cb42ae97242af0e70e6ad689012fcd13ee43ae32cc87f798eb5ed3295"] =
            Value::String(sudo_key);
    }

    if let Some(override_wasm) = relay.wasm_overrides() {
        let wasm_content = fs::read(override_wasm)
            .await
            .unwrap_or_else(|_| panic!("Error reading override_wasm from path {}", override_wasm));
        overrides["3a636f6465"] = Value::String(hex::encode(wasm_content));
    }

    // also check if any parachain includes a wasm override
    for para in paras {
        if let Some(override_wasm) = para.wasm_overrides() {
            let wasm_content = fs::read(override_wasm).await.unwrap_or_else(|_| {
                panic!("Error reading override_wasm from path {}", override_wasm)
            });
            let code_hash = hex::encode(subhasher::blake2_256(&wasm_content[..]));

            // we should now override
            let para_id_hash = crate::utils::para_id_hash(para.id());
            // Paras.CurrentCodeHash(paraId)
            let current_code_hash_prefix = array_bytes::bytes2hex(
                "",
                substorager::storage_value_key(&b"Paras"[..], b"CurrentCodeHash"),
            );
            overrides[&format!("{current_code_hash_prefix}{para_id_hash}")] =
                Value::String(code_hash.clone());

            // Paras.CodeByHash (should be injected since is have a reference to hash of the code itself)
            let code_by_hash_prefix = array_bytes::bytes2hex(
                "",
                substorager::storage_value_key(&b"Paras"[..], b"CodeByHash"),
            );
            let validation_code: ValidationCode = ValidationCode(wasm_content);
            let validation_code_encoded = validation_code.encode();
            injects[&format!("{code_by_hash_prefix}{code_hash}")] =
                Value::String(hex::encode(validation_code_encoded));

            // Paras.CodeByHashRefs (should be injected since is have a reference to hash of the code itself)
            let code_by_hash_prefix = array_bytes::bytes2hex(
                "",
                substorager::storage_value_key(&b"Paras"[..], b"CodeByHashRefs"),
            );
            // hardcoded to 1 encoded
            injects[&format!("{code_by_hash_prefix}{code_hash}")] =
                Value::String("01000000".into());
        }
    }

    let full_content = json!({
        "overrides": overrides,
        "injects": injects
    });

    let file_path = PathBuf::from(format!("{base_dir}/rc_overrides.json"));
    let contents = serde_json::to_string_pretty(&full_content).expect("Overrides should be valid.");
    fs::write(&file_path, contents)
        .await
        .expect("write file should works.");
    file_path
}

pub async fn generate_default_overrides_for_para(
    base_dir: &str,
    para: &Parachain,
    relay: &Relaychain,
) -> PathBuf {
    // asset-hub-polkadot use ed key
    let key_to_use = if relay.as_chain_string() == "polkadot" {
        "eb2f4b5e6f0bfa7ba42aa4b7eb2f43ba6c42061dbfc765bca066e51bb09f9116"
    } else {
        "005025ef7c9934c33534cbff35c9c5f0c1d30128e64f076c76942f49788eec15"
    };

    // Keys to inject (mostly storage maps that are not present in the current state)
    let injects = json!({
        // Session Nextkeys for `collator`
        "cec5070d609dd3497f72bde07fc96ba04c014e6bf8b8c2c011e7290b85696bb39af53646681828f1eb2f4b5e6f0bfa7ba42aa4b7eb2f43ba6c42061dbfc765bca066e51bb09f9116": "eb2f4b5e6f0bfa7ba42aa4b7eb2f43ba6c42061dbfc765bca066e51bb09f9116",
        "cec5070d609dd3497f72bde07fc96ba04c014e6bf8b8c2c011e7290b85696bb39af53646681828f1005025ef7c9934c33534cbff35c9c5f0c1d30128e64f076c76942f49788eec15": "005025ef7c9934c33534cbff35c9c5f0c1d30128e64f076c76942f49788eec15",

        // Session KeyOwner
        "cec5070d609dd3497f72bde07fc96ba0726380404683fc89e8233450c8aa1950eab3d4a1675d3d746175726180eb2f4b5e6f0bfa7ba42aa4b7eb2f43ba6c42061dbfc765bca066e51bb09f9116": "eb2f4b5e6f0bfa7ba42aa4b7eb2f43ba6c42061dbfc765bca066e51bb09f9116",
        "cec5070d609dd3497f72bde07fc96ba0726380404683fc89e8233450c8aa1950eab3d4a1675d3d746175726180005025ef7c9934c33534cbff35c9c5f0c1d30128e64f076c76942f49788eec15": "005025ef7c9934c33534cbff35c9c5f0c1d30128e64f076c76942f49788eec15",
    });

    // <Pallet> <Item>
    // e.g Validator Validators
    let mut overrides = json!({
        // Session Validators
        "cec5070d609dd3497f72bde07fc96ba088dcde934c658227ee1dfafcd6e16903": &format!("04{key_to_use}"),
        //	Session QueuedKeys
        "cec5070d609dd3497f72bde07fc96ba0e0cdd062e6eaf24295ad4ccfc41d4609": &format!("04{key_to_use}{key_to_use}"),
        // CollatorSelection Invulnerables (collator)
        "15464cac3378d46f113cd5b7a4d71c845579297f4dfb9609e7e4c2ebab9ce40a": &format!("04{key_to_use}"),
        // Aura authorities
        "57f8dc2f5ab09467896f47300f0424385e0621c4869aa60c02be9adcc98a0d1d": &format!("04{key_to_use}"),
        // AuraExt authorities
        "3c311d57d4daf52904616cf69648081e5e0621c4869aa60c02be9adcc98a0d1d": &format!("04{key_to_use}"),
        // parachainSystem lastDmqMqcHead (emtpy)
        "45323df7cc47150b3930e2666b0aa313911a5dd3f1155f5b7d0c5aa102a757f9": "0000000000000000000000000000000000000000000000000000000000000000",
        // CollatorSelection DesiredCandidates (set to 1)
        "15464cac3378d46f113cd5b7a4d71c84476f594316a7dfe49c1f352d95abdaf1": "01000000"
    });

    if let Some(override_wasm) = para.wasm_overrides() {
        let wasm_content = fs::read(override_wasm)
            .await
            .unwrap_or_else(|_| panic!("Error reading override_wasm from path {}", override_wasm));
        overrides["3a636f6465"] = Value::String(hex::encode(wasm_content));
    }

    let full_content = json!({
        "overrides": overrides,
        "injects": injects
    });

    let file_path = PathBuf::from(format!("{base_dir}/{}_overrides.json", para.id()));
    let contents = serde_json::to_string_pretty(&full_content).expect("Overrides should be valid.");
    fs::write(&file_path, contents)
        .await
        .expect("write file should works.");
    file_path
}

#[cfg(test)]
mod test {
    use super::generate_default_overrides_for_rc;

    #[tokio::test]
    async fn overrides_rc() {
        let paras = vec![];
        let _path = generate_default_overrides_for_rc(
            "/tmp",
            &crate::config::Relaychain::new("polakdot"),
            &paras,
        )
        .await;
    }
}

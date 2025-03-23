use crate::config::{Parachain, Relaychain};
use serde_json::{json, Value};
use std::path::PathBuf;
use tokio::fs;

pub async fn generate_default_overrides_for_rc(
    base_dir: &str,
    relay: &Relaychain,
    paras: &Vec<Parachain>,
) -> PathBuf {
    // Keys to inject (mostly storage maps that are not present in the current state)
    let mut injects = json!({});
    // <Pallet> < Item>
    // Validator Validators
    let mut overrides = json!({
        "7d9fe37370ac390779f35763d98106e888dcde934c658227ee1dfafcd6e16903": "08be5ddb1579b72e84524fc29e78609e3caf42e85aa118ebfe0b0ad404b5bdd25ffe65717dad0447d715f660a0a58411de509b42e6efb8375f562f58a554d5860e",
        // Session Validators (alice, bob)
        "cec5070d609dd3497f72bde07fc96ba088dcde934c658227ee1dfafcd6e16903": "08be5ddb1579b72e84524fc29e78609e3caf42e85aa118ebfe0b0ad404b5bdd25ffe65717dad0447d715f660a0a58411de509b42e6efb8375f562f58a554d5860e",
        //	Session QueuedKeys (alice, bob)
        "cec5070d609dd3497f72bde07fc96ba0e0cdd062e6eaf24295ad4ccfc41d4609": "08be5ddb1579b72e84524fc29e78609e3caf42e85aa118ebfe0b0ad404b5bdd25f88dc3417d5058ec4b4503e0c12ea1a0a89be200fe98922423d4334014fa6b0eed43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27dd43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27dd43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27dd43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d020a1091341fe5664bfa1782d5e04779689068c916b04cb365ec3153755684d9a1fe65717dad0447d715f660a0a58411de509b42e6efb8375f562f58a554d5860ed17c2d7823ebf260fd138f2d7e27d114c0145d968b5ff5006125f2414fadae698eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a488eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a488eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a488eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a480390084fdbf27d2b79d26a4f13f0ccd982cb755a661969143c37cbc49ef5b91f27",
        // Babe Authorities (alice, bob)
        "1cb6f36e027abb2091cfb5110ab5087f5e0621c4869aa60c02be9adcc98a0d1d": "08d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d01000000000000008eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a480100000000000000",
        // Babe NextAuthorities (alice, bob)
        "1cb6f36e027abb2091cfb5110ab5087faacf00b9b41fda7a9268821c2a2b3e4c": "08d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d01000000000000008eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a480100000000000000",
        // Grandpa Authorities (alice, bob)
        "5f9cc45b7a00c5899361e1c6099678dc5e0621c4869aa60c02be9adcc98a0d1d": "0888dc3417d5058ec4b4503e0c12ea1a0a89be200fe98922423d4334014fa6b0ee0100000000000000d17c2d7823ebf260fd138f2d7e27d114c0145d968b5ff5006125f2414fadae690100000000000000",
        // Staking ForceEra
        "5f3e4907f716ac89b6347d15ececedcaf7dad0317324aecae8744b87fc95f2f3": "02",
        // Staking Invulnerables (alice, bob)
        "5f3e4907f716ac89b6347d15ececedca5579297f4dfb9609e7e4c2ebab9ce40a": "08be5ddb1579b72e84524fc29e78609e3caf42e85aa118ebfe0b0ad404b5bdd25ffe65717dad0447d715f660a0a58411de509b42e6efb8375f562f58a554d5860e",
        // paras parachains (only 1000)
        "cd710b30bd2eab0352ddcc26417aa1940b76934f4cc08dee01012d059e1b83ee": "04e8030000",
        // paraScheduler validatorGroup (one group of 2 validators)
        "94eadf0156a8ad5156507773d0471e4a16973e1142f5bd30d9464076794007db": "041000000000010000000200000003000000",
        // paraScheduler claimQueue (empty, will auto-fill)
        "94eadf0156a8ad5156507773d0471e4a49f6c9aa90c04982c05388649310f22f": "040000000000",
        // paraShared activeValidatorIndices (2 validators)
        "b341e3a63e58a188839b242d17f8c9f82586833f834350b4d435d5fd269ecc8b": "080000000001000000",
        // paraShared activeValidatorKeys (alice, bob)
        "b341e3a63e58a188839b242d17f8c9f87a50c904b368210021127f9238883a6e": "08d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d8eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a48",
        // authorityDiscovery keys (alice, bob)
        "2099d7f109d6e535fb000bba623fd4409f99a2ce711f3a31b2fc05604c93f179": "08d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d8eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a48",
        // authorityDiscovery nextKeys (alice, bob)
        "2099d7f109d6e535fb000bba623fd4404c014e6bf8b8c2c011e7290b85696bb3": "08d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d8eaf04151687736326c9fea17e25fc5287613693c912909cb226aa4794f26a48",
        // Core descriptor, ensure core 0 is asset-hub
        "638595eebaa445ce03a13547bece90e704e6ac775a3245623103ffec2cb2c92fb4def25cfda6ef3ac02a707a7013b12ddc9c5f6a3e1994c51754be175bd6a3d4": "00010402e803000000e100e100010000e1",
        // dmp downwardMessageQueueHeads (empty for para 1000)
        "63f78c98723ddc9073523ef3beefda0c4d7fefc408aac59dbfe80a72ac8e3ce5b6ff6f7d467b87a9e8030000": "0000000000000000000000000000000000000000000000000000000000000000",
        // hrmp hrmpIngressChannelsIndex (empty for para 1000)
        "6a0da05ca59913bc38a8630590f2627c1d3719f5b0b12c7105c073c507445948b6ff6f7d467b87a9e8030000": "00",
        // Configuration activeConfig
        "06de3d8a54d27e44a9d5ce189618f22db4b49d95320d9021994c850f25b8e385": "0000300000500000aaaa020000001000fbff0000100000000a000000403800005802000003000000020000000000500000c800008000000000e8764817000000000000000000000000e87648170000000000000000000000e80300000090010080000000009001000c01002000000600c4090000000000000601983a00000000000040380000000600000058020000030000001900000000000000020000000200000002000000140000000100000008030100000014000000040000000105000000010000000100000000000000f401000080b2e60e80c3c90180b2e60e00000000000000000000000005000000",
        // paraScheduler availabilityCores (1 core, free)
        "94eadf0156a8ad5156507773d0471e4ab8ebad86f546c7e0b135a4212aace339": "0400",
        // Sudo Key (Alice)
        "5c0d1176a568c1f92944340dbfed9e9c530ebca703c85910e7164cb7d1c9e47b": "d43593c715fdd31c61141abd04a99fd6822c8558854ccde39a5684e7a56da27d"
    });

    if let Some(override_wasm) = relay.wasm_overrides() {
        let wasm_content = fs::read(override_wasm).await.expect(&format!(
            "Error reading override_wasm from path {}",
            override_wasm
        ));
        overrides["3a636f6465"] = Value::String(hex::encode(wasm_content));
    }

    // also check if any parachain includes a wasm override
    for para in paras {
        if let Some(override_wasm) = para.wasm_overrides() {
            let wasm_content = fs::read(override_wasm).await.expect(&format!(
                "Error reading override_wasm from path {}",
                override_wasm
            ));
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
            injects[&format!("{code_by_hash_prefix}{code_hash}")] =
                Value::String(hex::encode(wasm_content));

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

pub async fn generate_default_overrides_for_para(base_dir: &str, para: &Parachain) -> PathBuf {
    // Keys to inject (mostly storage maps that are not present in the current state)
    let injects = json!({});

    // <Pallet> < Item>
    // Validator Validators
    let mut overrides = json!({
        // Session Validators
        "cec5070d609dd3497f72bde07fc96ba088dcde934c658227ee1dfafcd6e16903": "04005025ef7c9934c33534cbff35c9c5f0c1d30128e64f076c76942f49788eec15",
        //	Session QueuedKeys
        "cec5070d609dd3497f72bde07fc96ba0e0cdd062e6eaf24295ad4ccfc41d4609": "04005025ef7c9934c33534cbff35c9c5f0c1d30128e64f076c76942f49788eec15eb2f4b5e6f0bfa7ba42aa4b7eb2f43ba6c42061dbfc765bca066e51bb09f9116",
        // Session keys for `collator`
        "cec5070d609dd3497f72bde07fc96ba04c014e6bf8b8c2c011e7290b85696bb39af53646681828f1005025ef7c9934c33534cbff35c9c5f0c1d30128e64f076c76942f49788eec15": "eb2f4b5e6f0bfa7ba42aa4b7eb2f43ba6c42061dbfc765bca066e51bb09f9116",
        "cec5070d609dd3497f72bde07fc96ba0726380404683fc89e8233450c8aa1950eab3d4a1675d3d746175726180eb2f4b5e6f0bfa7ba42aa4b7eb2f43ba6c42061dbfc765bca066e51bb09f9116": "005025ef7c9934c33534cbff35c9c5f0c1d30128e64f076c76942f49788eec15",
        // CollatorSelection Invulnerables
        "15464cac3378d46f113cd5b7a4d71c845579297f4dfb9609e7e4c2ebab9ce40a": "044cec53d80585625c427e909070de80016e629fa02e5cb373f3c4e94226417726",
        // Aura authorities
        "57f8dc2f5ab09467896f47300f0424385e0621c4869aa60c02be9adcc98a0d1d": "04eb2f4b5e6f0bfa7ba42aa4b7eb2f43ba6c42061dbfc765bca066e51bb09f9116",
        // AuraExt authorities
        "3c311d57d4daf52904616cf69648081e5e0621c4869aa60c02be9adcc98a0d1d": "04eb2f4b5e6f0bfa7ba42aa4b7eb2f43ba6c42061dbfc765bca066e51bb09f9116",
        // parachainSystem lastDmqMqcHead (emtpy)
        "45323df7cc47150b3930e2666b0aa313911a5dd3f1155f5b7d0c5aa102a757f9": "0000000000000000000000000000000000000000000000000000000000000000",
    });

    if let Some(override_wasm) = para.wasm_overrides() {
        let wasm_content = fs::read(override_wasm).await.expect(&format!(
            "Error reading override_wasm from path {}",
            override_wasm
        ));
        overrides["3a636f6465"] = Value::String(hex::encode(wasm_content));
    }

    let full_content = json!({
        "overrides": overrides,
        "injects": injects
    });

    let file_path = PathBuf::from(format!(
        "{base_dir}/{}_overrides.json",
        para.id()
    ));
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
            &crate::config::Relaychain::Polkadot(None),
            &paras,
        )
        .await;
    }
}

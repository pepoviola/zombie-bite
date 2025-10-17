#![allow(dead_code)]
// TODO: don't allow dead_code

use std::path::{Path, PathBuf};
use std::str::FromStr;

use anyhow::anyhow;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sp_core::bytes;
use tokio::fs::{self, File};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use codec::{CompactAs, Decode, Encode, MaxEncodedLen};
use tracing::trace;

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

/// Parachain head data included in the chain.
#[derive(PartialEq, Eq, Clone, PartialOrd, Ord, Encode, Decode)]
pub struct HeadData(pub Vec<u8>);

/// Parachain validation code.
#[derive(PartialEq, Eq, Clone, Encode, Decode, Serialize, Deserialize)]
pub struct ValidationCode(#[serde(with = "bytes")] pub Vec<u8>);

pub async fn get_random_port() -> u16 {
    let listener = TcpListener::bind("0.0.0.0:0".to_string())
        .await
        .expect("Can't bind a random port");

    listener
        .local_addr()
        .expect("We should always get the local_addr from the listener, qed")
        .port()
}

/// Read the file's content into a [`Vec<u8>`].
async fn read_file_to_vec<P>(path: P) -> Result<Vec<u8>, anyhow::Error>
where
    P: AsRef<Path>,
{
    let mut file = tokio::fs::File::open(&path)
        .await
        .map_err(|_| anyhow!("Error opening file {}", path.as_ref().to_string_lossy()))?;
    let mut content = Vec::new();

    file.read_to_end(&mut content)
        .await
        .map_err(|_| anyhow!("Error reading file {}", path.as_ref().to_string_lossy()))?;

    Ok(content)
}

/// Read the file's content into a struct implemented [`DeserializeOwned`].
pub async fn read_file_to_struct<P, T>(path: P) -> Result<T, anyhow::Error>
where
    P: AsRef<Path>,
    T: DeserializeOwned,
{
    let content = read_file_to_vec(&path).await?;

    let result = serde_json::from_slice(&content).map_err(|_| {
        anyhow!(
            "Error deserializing  file {}",
            path.as_ref().to_string_lossy()
        )
    })?;

    Ok(result)
}

/// Write the data to file at the given path.
///
/// If the file has already existed, then it will be overwritten.
/// Otherwise, this will create a file at the given path.
pub async fn write_data_to_file<P>(path: P, data: &[u8]) -> Result<(), anyhow::Error>
where
    P: AsRef<Path>,
{
    let mut file = File::create(&path)
        .await
        .map_err(|_| anyhow!("Error creating file {}", path.as_ref().to_string_lossy()))?;

    file.write_all(data)
        .await
        .map_err(|_| anyhow!("Error writting file {}", path.as_ref().to_string_lossy()))
}

pub fn para_head_key(para_id: u32) -> String {
    const PARAS_HEAD_PREFIX: &str =
        "0xcd710b30bd2eab0352ddcc26417aa1941b3c252fcb29d88eff4f3de5de4476c3";
    // let para_id: ParaId = para_id.into();
    let para_id_hash = para_id_hash(para_id);
    format!("{PARAS_HEAD_PREFIX}{para_id_hash}")
    // subhasher::twox64_concat(&para_id.encode());
    // let key = format!(
    //     "{PARAS_HEAD_PREFIX}{}",
    //     array_bytes::bytes2hex("", &para_id_hash)
    // );

    // key
}

/// Returns the hash of the ParaId (without the 0x prefix)
pub fn para_id_hash(para_id: u32) -> String {
    let para_id: ParaId = para_id.into();
    let para_id_hash = subhasher::twox64_concat(para_id.encode());
    array_bytes::bytes2hex("", &para_id_hash)
}

pub async fn localize_config(config_path: impl AsRef<str>) -> Result<(), anyhow::Error> {
    let config_path = PathBuf::from_str(config_path.as_ref())?;
    let base_path = config_path.parent().unwrap();

    let mut localized = false;

    // read config
    let config_content = fs::read_to_string(&config_path)
        .await
        .expect("read config should works");
    let mut config_modified = vec![];
    for line in config_content.lines() {
        match line {
            l if l.starts_with("default_db_snapshot")
                | l.starts_with("chain_spec_path")
                | l.starts_with("db_snapshot") =>
            {
                let parts: Vec<&str> = l.split("=").collect();
                let value_as_path = PathBuf::from_str(parts.last().unwrap())
                    .expect(&format!("value {:?} should be a valid path", parts.last()));
                let maybe_mod_line = if let Ok(false) = fs::try_exists(&value_as_path).await {
                    // localize!
                    localized = true;
                    let mod_line = format!(
                        r#"{} = "{}/{}"#,
                        parts.first().unwrap().trim(),
                        base_path.to_string_lossy(),
                        value_as_path.file_name().unwrap().to_string_lossy()
                    );
                    trace!("localize line from: {l} to {mod_line}");
                    mod_line
                } else {
                    l.to_string()
                };

                config_modified.push(maybe_mod_line)
            }
            l if l.starts_with("base_dir") => {
                localized = true;
                // remove base_path
            }
            _ => {
                config_modified.push(line.to_string());
            }
        }
    }

    if localized {
        // rename original
        fs::rename(
            &config_path,
            &format!("{}/original-config.toml", &base_path.to_string_lossy()),
        )
        .await
        .expect("rename should works");
        fs::write(&config_path, config_modified.join("\n"))
            .await
            .expect("write should works");
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn para_head_key_should_work() {
        let para_id = 1000_u32;
        let head_key = para_head_key(para_id);
        assert_eq!(&head_key, "0xcd710b30bd2eab0352ddcc26417aa1941b3c252fcb29d88eff4f3de5de4476c3b6ff6f7d467b87a9e8030000");
    }

    #[tokio::test]
    async fn localize_config_should_works() {
        let config_path = "./testing/config.toml";
        let config_path_bkp = "./testing/config.toml.bkp";
        let _ = fs::copy(&config_path_bkp, config_path).await;
        let _ = localize_config(config_path).await.unwrap();
    }

    #[tokio::test]
    async fn localize_paseo_config_should_works() {
        tracing_subscriber::fmt::init();
        let config_path = "./testing/config-paseo.toml";
        let config_path_bkp = "./testing/config-paseo.toml.bkp";
        let _ = fs::copy(&config_path_bkp, config_path).await;
        let _ = localize_config(config_path).await.unwrap();
        let network_config =
            zombienet_configuration::NetworkConfig::load_from_toml(&config_path).unwrap();
        let alice_db = network_config
            .relaychain()
            .nodes()
            .first()
            .unwrap()
            .db_snapshot()
            .unwrap()
            .to_string();

        assert!(alice_db.contains("./testing"));
    }
}

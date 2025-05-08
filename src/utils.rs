#![allow(dead_code)]
// TODO: don't allow dead_code

use std::path::Path;

use anyhow::anyhow;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sp_core::bytes;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

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

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn para_head_key_should_work() {
        let para_id = 1000_u32;
        let head_key = para_head_key(para_id);
        assert_eq!(&head_key, "0xcd710b30bd2eab0352ddcc26417aa1941b3c252fcb29d88eff4f3de5de4476c3b6ff6f7d467b87a9e8030000");
    }
}

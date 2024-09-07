use std::path::Path;

use anyhow::anyhow;
use serde::de::DeserializeOwned;
use tokio::fs::File;
use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt,AsyncWriteExt};


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
	let mut file = tokio::fs::File::open(&path).await.map_err(|_| anyhow!("Error opening file {}", path.as_ref().to_string_lossy()))?;
	let mut content = Vec::new();

	file.read_to_end(&mut content).await.map_err(|_| anyhow!("Error reading file {}", path.as_ref().to_string_lossy()))?;

	Ok(content)
}


/// Read the file's content into a struct implemented [`DeserializeOwned`].
pub async fn read_file_to_struct<P, T>(path: P) -> Result<T, anyhow::Error>
where
	P: AsRef<Path>,
	T: DeserializeOwned,
{

	let content = read_file_to_vec(&path).await?;

	let result = serde_json::from_slice(&content).map_err(|_| anyhow!("Error deserializing  file {}", path.as_ref().to_string_lossy()))?;

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
	let mut file = File::create(&path).await.map_err(|_| anyhow!("Error creating file {}", path.as_ref().to_string_lossy()))?;

	Ok(file.write_all(data).await.map_err(|_| anyhow!("Error writting file {}", path.as_ref().to_string_lossy()))?)
}
//! This module contains utils functions.

use std::fs::File;
use std::io::Write;
use std::path::Path;

use futures_util::StreamExt;

use reqwest::Client;

use crate::Result;

/// Downloads a file to a place on the disk.
pub async fn download<P: AsRef<Path>>(url: &str, target: P) -> Result<()> {
    let target = target.as_ref();

    let client = Client::new();

    let req = client.get(url).send().await?;

    let mut file = File::create(&target)?;
    let mut byte_stream = req.bytes_stream();

    while let Some(chunk) = byte_stream.next().await {
        let bytes = chunk?;
        file.write_all(&bytes)?;
    }

    Ok(())
}

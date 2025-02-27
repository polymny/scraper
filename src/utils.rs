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

/// Returns the name of the entry, without the author.
pub fn pretty_name(valid_name: &str) -> Option<String> {
    let split = valid_name.replace("(", "").replace(")", "");
    let split = split.split_whitespace().collect::<Vec<_>>();

    // Found the author: its the second word that starts with uppercase character
    let author_index = split
        .iter()
        .skip(1)
        .position(|x| x.chars().any(char::is_uppercase))?
        + 1;

    // Join everything until author
    Some(split[0..author_index].join(" "))
}

/// Returns the author of the entry.
pub fn pretty_finder(valid_name: &str) -> Option<String> {
    let split = valid_name.replace("(", "").replace(")", "");
    let split = split.split_whitespace().collect::<Vec<_>>();

    // Found the author: its the second word that starts with uppercase character
    let author_index = split
        .iter()
        .skip(1)
        .position(|x| x.chars().any(char::is_uppercase))?
        + 1;

    // Join everything until author
    Some(split[author_index..].join(" "))
}

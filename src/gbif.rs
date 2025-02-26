//! This module contains all the functions that help us use the GBIF API.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use serde_json::Value;

use uuid::{uuid, Uuid};

use unidecode::unidecode;

use tokio::time::sleep;

use crate::Result;

/// GBIF Backbone dataset where we will search for species.
pub const BACKBONE_DATASET_UUID: Uuid = uuid!("d7dddbf4-2cf0-4f39-9b2a-bb099caae36c");

/// Root of the GBIF API server.
pub const GBIF_ROOT: &'static str = "https://api.gbif.org/v1";

/// Maximum number of occurrences that can be scraped.
pub const MAX_LIMIT_OCCURRENCES: usize = 300;

/// The complete response of a GBIF species search query.
#[derive(Debug, Serialize, Deserialize)]
pub struct SpeciesResponse {
    /// All results that match the query.
    pub results: Vec<SpeciesResult>,
}

/// The complete response of a GBIF species search query.
#[derive(Debug, Serialize, Deserialize)]
pub struct SpeciesOptionalResponse {
    /// All results that match the query.
    pub results: Vec<SpeciesOptionalResult>,
}

/// A single result of a GBIF search query.
#[derive(Debug, Serialize, Deserialize)]
pub struct SpeciesResult {
    /// The species key of the species.
    #[serde(rename = "speciesKey")]
    pub species_key: i64,

    /// The scientific name of the species (both name and author).
    #[serde(rename = "scientificName")]
    pub scientific_name: String,
}

/// A single result of a GBIF search query.
#[derive(Debug, Serialize, Deserialize)]
pub struct SpeciesOptionalResult {
    /// The species key of the species.
    #[serde(rename = "speciesKey")]
    pub species_key: Option<i64>,

    /// The scientific name of the species (both name and author).
    #[serde(rename = "scientificName")]
    pub scientific_name: String,
}

impl SpeciesOptionalResult {
    /// Converts SpeciesOptionalResult to Option<SpeciesResult>.
    pub fn into_option(self) -> Option<SpeciesResult> {
        if let Some(species_key) = self.species_key {
            Some(SpeciesResult {
                species_key,
                scientific_name: self.scientific_name,
            })
        } else {
            None
        }
    }
}

/// Easily create a gbif api url.
pub fn gbif_url(suffix: &str) -> String {
    format!("{}{}", GBIF_ROOT, suffix)
}

/// Preprocesses a string for better query.
pub fn preprocess(input: &str) -> String {
    unidecode(input)
        .replace("(", "")
        .replace(")", "")
        .replace(",", "")
        .to_lowercase()
}

/// Searches a name of a species on GBIF and returns it.
pub async fn search_species(species: &str) -> Result<SpeciesResponse> {
    let response = reqwest::get(gbif_url(&format!(
        "/species/search?q={}&limit=300&datasetKey={}",
        preprocess(species),
        BACKBONE_DATASET_UUID,
    )))
    .await?
    .text()
    .await?;

    let response: SpeciesOptionalResponse = serde_json::from_str(&response)?;

    // Remove responses without species key
    Ok(SpeciesResponse {
        results: response
            .results
            .into_iter()
            .filter_map(SpeciesOptionalResult::into_option)
            .collect(),
    })
}

/// The complete response of a GBIF occurrences search query.
#[derive(Serialize, Deserialize)]
pub struct OccurrencesResponse {
    /// All results that match the query.
    pub results: Vec<OccurrencesResult>,

    /// The total number of occurrences available on GBIF.
    pub count: i64,
}

/// The results from an occurrences search.
#[derive(Serialize, Deserialize)]
pub struct OccurrencesResult {
    /// GBIF id of the occurrence.
    pub key: i64,

    /// UUID of the dataset in which the occurrence is.
    #[serde(rename = "datasetKey")]
    pub dataset_key: Uuid,

    /// Medias that come with this occurrence.
    #[serde(rename = "media")]
    pub medias: Vec<Media>,
}

/// A media representing the occurrence.
#[derive(Serialize, Deserialize)]
pub struct Media {
    /// The URL of the media.
    #[serde(rename = "identifier")]
    pub url: Option<String>,
}

/// Search occurrences for a species.
pub async fn search_occurrences(species_key: i64, offset: usize, limit: usize) -> Result<Value> {
    let mut retries = 3;

    loop {
        trace!(
            "looking up occurrences for {} (offset={}, limit={}, attempt={})",
            species_key,
            offset,
            limit,
            4 - retries
        );

        let url = gbif_url(&format!(
            "/occurrence/search?speciesKey={}&offset={}&limit={}&mediaType=stillImage",
            species_key, offset, limit,
        ));

        let response = reqwest::get(url).await?;

        let code = response.status().as_u16();
        let text = response.text().await?;

        if 200 <= code && code < 400 {
            return Ok(serde_json::from_str(&text)?);
        }

        if code == 429 && retries > 0 {
            retries -= 1;
            trace!("Received 429 Too Many Requests from GBIF, waiting a little");
            sleep(Duration::from_secs(5)).await;
            continue;
        } else {
            // Ideally we should return an error, but here, the from_str will fail, so it's not
            // that big a deal.
            return Ok(serde_json::from_str(&text)?);
        }
    }
}

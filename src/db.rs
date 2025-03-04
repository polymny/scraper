//! This module helps us deal with the database.

use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use serde::Serialize;

use serde_json::{json, Value};

use tokio::time::sleep;

use ergol::prelude::*;
use ergol::tokio_postgres::GenericClient;

use uuid::Uuid;

use futures_util::StreamExt;

use reqwest::Client;

use infer::MatcherType;

use crate::config::Storage;
use crate::gbif::{search_occurrences, search_species, OccurrencesResponse, MAX_LIMIT_OCCURRENCES};
use crate::taxref::Entry;
use crate::utils::{pretty_finder, pretty_name};
use crate::{Db, Error, Result};

/// A species that is ignored because we already have another species with the same species key in the database.
#[ergol]
pub struct IgnoredSpecies {
    /// Id of the row in the database.
    #[id]
    pub id: i32,

    /// Reign of the species.
    pub reign: String,

    /// Phylum of the species.
    pub phylum: String,

    /// Class of the species.
    pub class: String,

    /// Order of the species.
    pub order: String,

    /// Family of the species.
    pub family: String,

    /// Genus of the species (sub-family).
    pub genus: String,

    /// Valid name of the specie.
    #[unique]
    pub valid_name: String,

    /// The species key of the species on GBIF.
    pub species_key: Option<i64>,
}

impl IgnoredSpecies {
    /// Prepares an ignored species without id from its taxref entry.
    pub fn from_taxref(entry: Entry, species_key: Option<i64>) -> IgnoredSpeciesWithoutId {
        IgnoredSpeciesWithoutId {
            reign: entry.reign,
            phylum: entry.phylum,
            class: entry.class,
            order: entry.order,
            family: entry.family,
            genus: entry.genus,
            valid_name: entry.valid_name,
            species_key,
        }
    }
}

/// A species that is registered in the database.
#[ergol]
#[derive(Serialize)]
pub struct Species {
    /// Id of the row in the database.
    #[id]
    pub id: i32,

    /// Reign of the species.
    pub reign: String,

    /// Phylum of the species.
    pub phylum: String,

    /// Class of the species.
    pub class: String,

    /// Order of the species.
    pub order: String,

    /// Family of the species.
    pub family: String,

    /// Genus of the species (sub-family).
    pub genus: String,

    /// Valid name of the specie.
    #[unique]
    pub valid_name: String,

    /// The species key of the species on GBIF.
    #[unique]
    pub species_key: Option<i64>,

    /// The number of available occurrences for the species.
    pub available_occurrences: i64,

    /// Whether the scraping is done for this species.
    pub done: bool,
}

impl Species {
    /// Returns a meaningful representation of the species in JSON.
    pub async fn to_json(&self, db: &Db) -> Result<Value> {
        Ok(json!({
            "reign": self.reign,
            "phylum": self.phylum,
            "class": self.class,
            "order": self.order,
            "family": self.family,
            "genus": self.genus,
            "valid_name": self.valid_name,
            "pretty_name": pretty_name(&self.valid_name),
            "pretty_finder": pretty_finder(&self.valid_name),
            "occurrences": self.occurrences(db).await?,
            "species_key": self.species_key,
        }))
    }
    /// Prepares a species without id from its taxref entry.
    pub fn from_taxref(
        entry: Entry,
        species_key: Option<i64>,
        available_occurrences: i64,
    ) -> SpeciesWithoutId {
        SpeciesWithoutId {
            reign: entry.reign,
            phylum: entry.phylum,
            class: entry.class,
            order: entry.order,
            family: entry.family,
            genus: entry.genus,
            valid_name: entry.valid_name,
            species_key,
            available_occurrences,
            done: false,
        }
    }

    /// Searches the species on gbif and scrap its occurrences.
    pub async fn scrap_occurrences<T: Queryable<impl GenericClient>>(
        species: Entry,
        max_occurrences: usize,
        blacklist: &Uuid,
        storage: &Storage,
        db: &T,
    ) -> Result<Species> {
        // Check if species is already in the db.
        let db_species = Species::get_by_valid_name(&species.valid_name, db).await?;

        let species_key = match db_species {
            // If scraping is already finished, early return.
            Some(x) if x.done => return Ok(x),

            // If not, continue scraping from species_key in db.
            Some(x) => x.species_key,

            // First time scraping the species, or already scraped but not saved because duplicate.
            None => {
                let db_species = IgnoredSpecies::get_by_valid_name(&species.valid_name, db).await?;
                if db_species.is_some() {
                    // Early return if it was ignored.
                    return Err(Error::SpeciesNotFound(species.valid_name.clone()));
                } else {
                    None
                }
            }
        };

        let species_key = if let Some(species_key) = species_key {
            species_key
        } else {
            let pretty = pretty_name(&species.valid_name).expect(&format!(
                "Failed to extract pretty name from \"{}\"",
                species.valid_name,
            ));
            let gbif_response = search_species(&pretty).await?;

            if let Some(r) = gbif_response.results.first() {
                r.species_key
            } else {
                warn!("species {} not found", species.valid_name);

                // Save species with no species key.
                Species::from_taxref(species, None, 0).save(db).await?;
                return Err(Error::SpeciesNotFound(pretty));
            }
        };

        // Look if there already is a species with the same species key in the database.
        let duplicate = Species::get_by_species_key(species_key, db).await?;

        if let Some(duplicate) = duplicate {
            IgnoredSpecies::from_taxref(species, Some(species_key))
                .save(db)
                .await?;

            return Ok(duplicate);
        }

        // Start scraping occurrences.
        let mut json_occurrences =
            search_occurrences(species_key, 0, MAX_LIMIT_OCCURRENCES).await?;

        let mut parsed_occurrences: OccurrencesResponse =
            serde_json::from_value(json_occurrences.clone())?;

        let mut count = parsed_occurrences.results.len();

        // Now that we know the total number of occurrences available, we can store the species in the database.
        let mut db_species =
            Species::from_taxref(species, Some(species_key), parsed_occurrences.count)
                .save(db)
                .await?;

        // Count non blacklisted occurrences that have medias.
        let mut scraped = parsed_occurrences
            .results
            .iter()
            .filter(|x| x.dataset_key != *blacklist)
            .filter(|x| !x.medias.is_empty())
            .count();

        // If we don't have enough occurrences, fetch more.
        while scraped < max_occurrences && count < parsed_occurrences.count as usize {
            let current = search_occurrences(species_key, count, MAX_LIMIT_OCCURRENCES).await?;

            let parsed: OccurrencesResponse = serde_json::from_value(current.clone())?;
            count += parsed.results.len();

            // Count non blacklisted occurrences that have medias.
            scraped += &parsed
                .results
                .iter()
                .filter(|x| x.dataset_key != *blacklist)
                .filter(|x| !x.medias.is_empty())
                .count();

            // Append results to parsed occurrences.
            parsed_occurrences.results.extend(parsed.results);

            // Append results to parsed json.
            // Note: these unwraps are ok because if they failed, the parsing of the json would have failed earlier.
            json_occurrences
                .get_mut("results")
                .unwrap()
                .as_array_mut()
                .unwrap()
                .extend(current.get("results").unwrap().as_array().unwrap().clone());
        }

        // Save json in species data dir.
        let mut json_file =
            File::create(storage.species_dir().join(format!("{}.json", species_key)))?;

        json_file.write_all(serde_json::to_string_pretty(&json_occurrences)?.as_bytes())?;

        // Save occurrences and media in db.
        'outer: for result in &parsed_occurrences.results {
            // We only want to save occurrences that do not have media already present in the database (GBIF contains
            // duplicates that we want to avoid).
            for media in &result.medias {
                if let Some(url) = &media.url {
                    if Media::get_by_url(url, db).await?.is_some() {
                        // We found a duplicate, so we continue outer loop, i.e. go to next result.
                        continue 'outer;
                    }
                }
            }

            // We have no longer duplicates: insert occurrences and medias in database.
            let occurrence = Occurrence::create(result.key, result.dataset_key, &db_species)
                .save(db)
                .await?;

            for media in &result.medias {
                let url = if let Some(url) = media.url.as_ref() {
                    url
                } else {
                    continue;
                };

                Media::new(url, &occurrence).save(db).await?;
            }
        }

        db_species.done = true;
        db_species.save(db).await?;

        Ok(db_species)
    }
}

/// An occurrence of a species.
#[ergol]
#[derive(Serialize)]
pub struct Occurrence {
    /// Id of the row in the database.
    #[id]
    pub id: i32,

    /// Id of the occurrence in GBIF.
    #[unique]
    pub key: i64,

    /// UUID of the dataset that contains the occurrence.
    pub dataset_key: Uuid,

    /// The species that correspond to this occurrence.
    #[many_to_one(occurrences)]
    #[serde(skip)]
    pub species: Species,
}

/// A media of an occurrence.
#[ergol]
#[derive(Serialize)]
pub struct Media {
    /// Id of the row in the database
    #[id]
    pub id: i32,

    /// The url of the media.
    #[unique]
    pub url: String,

    /// The path with which we will store the media.
    ///
    /// It allows us to rebuild the media path with {medias_dir}/{path}
    pub path: Option<String>,

    /// Status code returned when the file was downloaded, None if no attempt to download was made.
    pub status_code: Option<i32>,

    /// True if we want to download the media.
    pub to_download: bool,

    /// Whether a crop was attempted.
    ///
    /// If this value is true, and x, y, width and height are still none, it means that the cropping has failed.
    pub cropped: bool,

    /// x coordinate of the bounding box of the cropping.
    pub x: Option<f64>,

    /// y coordinate of the bounding box of the cropping.
    pub y: Option<f64>,

    /// Width of the bounding box of the cropping.
    pub width: Option<f64>,

    /// Height of the bounding box of the cropping.
    pub height: Option<f64>,

    /// Confidence of the cropping.
    pub confidence: Option<f64>,

    /// Occurrence that references this media.
    #[many_to_one(medias)]
    #[serde(skip)]
    pub occurrence: Occurrence,
}

impl Media {
    /// Creates a media with good default values.
    pub fn new(url: &str, occurrence: &Occurrence) -> MediaWithoutId {
        Media::create(
            url.to_owned(),
            None,
            None,
            false,
            false,
            None,
            None,
            None,
            None,
            None,
            occurrence,
        )
    }

    /// Downloads the media.
    pub async fn download<Q: Queryable<impl GenericClient>>(
        &mut self,
        client: &Client,
        storage: &Storage,
        db: &Q,
    ) -> Result<i32> {
        if let Some(status_code) = self.status_code {
            // Skip download
            return Ok(status_code);
        }

        let occurrence = self.occurrence(db).await?;
        let species = occurrence.species(db).await?;
        self.download_with_info(&occurrence, &species, client, storage, db)
            .await
    }

    /// Returns the download path of the media if the media has been downloaded, none otherwise.
    pub fn path(&self, config: &Storage) -> Option<String> {
        if let Some(path) = &self.path {
            Some(format!(
                "{}/{}",
                config
                    .data_path
                    .to_str()
                    .expect("Failed to convert path to str"),
                path.clone()
            ))
        } else {
            None
        }
    }

    /// Downloads the media by specifying its occurrence and species, and edit the database to
    /// store its info.
    pub async fn download_with_info<Q: Queryable<impl GenericClient>>(
        &mut self,
        occurrence: &Occurrence,
        species: &Species,
        client: &Client,
        storage: &Storage,
        db: &Q,
    ) -> Result<i32> {
        let mut retries = 0;

        let (code, target_local) = loop {
            let download = self
                .download_dirty_with_info(occurrence, species, client, storage)
                .await;

            let (code, extension) = download.ok().unwrap_or((600, None));

            if code == 429 && retries > 0 {
                // Too many requests, wait a little bit, and try again
                trace!(
                    "Received 429 for {} {}, waiting a little bit (attempt={})",
                    self.id,
                    self.url,
                    4 - retries,
                );
                retries -= 1;
                sleep(Duration::from_secs(10)).await;
            } else {
                break (code, extension);
            }
        };

        self.status_code = Some(code);
        if let Some(target_local) = target_local {
            self.path = Some(
                target_local
                    .to_str()
                    .expect("Failed to convert path to str, this should never happen")
                    .to_string(),
            );
        }
        self.save(db).await?;

        Ok(code)
    }
    /// Downloads the media by specifying its occurrence and species.
    async fn download_dirty_with_info(
        &self,
        occurrence: &Occurrence,
        species: &Species,
        client: &Client,
        storage: &Storage,
    ) -> Result<(i32, Option<PathBuf>)> {
        let species_key = if let Some(species_key) = species.species_key {
            species_key
        } else {
            return Err(Error::SpeciesNotFound(species.valid_name.clone()));
        };

        // We don't know the extension yet, because we don't know the type of image.
        let mut target_local = storage
            .medias_dir_local(species_key)
            .join(format!("{}_{:04}", occurrence.key, self.id));

        let mut target = storage
            .medias_dir(species_key)
            .join(format!("{}_{:04}", occurrence.key, self.id));

        // Start downloading.
        let req = client.get(&self.url).send().await?;
        let status = req.status();
        let code = status.as_u16() as i32;

        let target = if status.is_success() {
            let mut byte_stream = req.bytes_stream();

            // Download first chunk to find magic numbers, mime type and extension.
            let chunk = byte_stream.next().await;

            let bytes = if let Some(chunk) = chunk {
                chunk
            } else {
                return Err(Error::DownloadFailed(self.url.clone()));
            }?;

            // Find mime type and extension.
            let ty = infer::get(&bytes).ok_or(Error::UnknownMediaType(self.url.clone()))?;

            if ty.matcher_type() != MatcherType::Image {
                return Err(Error::UnknownMediaType(self.url.clone()));
            }

            // Set the extension accordingly.
            target.set_extension(ty.extension());
            target_local.set_extension(ty.extension());

            let mut file = File::create(&target)?;

            // Write first chunk.
            file.write_all(&bytes)?;

            // Perform rest of downloading.
            while let Some(chunk) = byte_stream.next().await {
                let bytes = chunk?;
                file.write_all(&bytes)?;
            }

            Some(target_local)
        } else {
            None
        };

        Ok((code, target))
    }

    /// Returns true if a media was successfully downloaded.
    pub fn is_downloaded(&self) -> bool {
        match self.status_code {
            Some(i) if 200 <= i && i < 300 => true,
            _ => false,
        }
    }
}

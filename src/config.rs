//! This module contains the struct useful for the configuration.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use rocket::figment::Figment;
use rocket::Phase;

/// The databases of the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Databases {
    /// The database of the server.
    pub database: Database,
}

/// The url of the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Database {
    /// The url of the database.
    pub url: String,
}

/// The config of the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// The root of the app.
    pub root: String,

    /// The path where the videos will be published.
    #[serde(flatten, rename = "data_path")]
    pub storage: Storage,

    /// Number of parallel downloads.
    pub jobs: usize,

    /// Number of images to put in a batch for cropping.
    pub batch_size: usize,

    /// Url of the databases.
    pub databases: Databases,
}

impl Config {
    /// Creates the config struct from the rocket config.
    pub fn from_rocket<P: Phase>(rocket: &rocket::Rocket<P>) -> Config {
        Config::from_figment(rocket.figment())
    }

    /// Creates the config struct from the rocket figment.
    pub fn from_figment(figment: &Figment) -> Config {
        figment.extract().expect("Failed to parse config")
    }
}

/// Struct to help us deal with storage paths.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Storage {
    /// Path where the data is stored.
    pub data_path: PathBuf,
}

impl Storage {
    /// Returns the path to the species directory.
    pub fn species_dir(&self) -> PathBuf {
        self.data_path.join("species")
    }

    /// Returns the media path for a species.
    pub fn medias_dir(&self, species_key: i64) -> PathBuf {
        self.data_path
            .join("medias")
            .join(&self.medias_dir_local(species_key))
    }

    /// Returns the part of the media path after medias.
    pub fn medias_dir_local(&self, species_key: i64) -> PathBuf {
        PathBuf::from(format!("{}", species_key))
    }
}

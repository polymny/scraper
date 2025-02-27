//! Scraper for GBIF data.

#![warn(missing_docs)]

#[macro_use]
extern crate rocket;

pub mod config;
pub mod cropper;
pub mod db;
pub mod gbif;
pub mod logger;
pub mod server;
pub mod taxref;
pub mod utils;

use std::env::args;
use std::fs::{create_dir_all, File};
use std::process::exit;
use std::result::Result as StdResult;
use std::time::Duration;
use std::{fmt, io};

use chrono::prelude::*;

use tokio::sync::mpsc::unbounded_channel;
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;

use rocket::http::Status;
use rocket::request::{FromRequest, Outcome, Request};
use rocket::State;

use ergol::deadpool::managed::Object;
use ergol::prelude::*;
use ergol::tokio_postgres::Error as TpError;
use ergol::Pool;

use crate::config::{Config, BLACKLISTED_DATASET};
use crate::cropper::Cropper;
use crate::db::{Media, Species};
use crate::logger::Log;
use crate::taxref::{Entry, Taxon};

/// The error type of this library.
#[derive(Debug)]
pub enum Error {
    /// An HTTP request failed.
    ReqwestError(reqwest::Error),

    /// An IO error occured.
    IoError(io::Error),

    /// An error occurred with postgresql.
    PostgresError(TpError),

    /// An error while parsing json.
    JsonError(serde_json::Error),

    /// Cache directory does not exist.
    NoCache,

    /// Error in db.
    DbError,

    /// Download failed.
    DownloadFailed(String),

    /// We attempted to download a file which media type is not supported.
    UnknownMediaType(String),

    /// The species was not found on GBIF.
    SpeciesNotFound(String),

    /// Failed to initialize cropper.
    InitializeCropperFailed,

    /// An error with the rocket web framework.
    RocketError(rocket::Error),

    /// An internal server error for our server.
    InternalServerError,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::ReqwestError(e) => write!(f, "{}", e),
            Error::IoError(e) => write!(f, "{}", e),
            Error::PostgresError(e) => write!(f, "{}", e),
            Error::JsonError(e) => write!(f, "{}", e),
            Error::NoCache => write!(f, "couldn't find cache directory"),
            Error::DbError => write!(f, "error with database"),
            Error::DownloadFailed(file) => write!(f, "failed to download file: {}", file),
            Error::UnknownMediaType(file) => write!(f, "unknown media type for file: {}", file),
            Error::SpeciesNotFound(species) => {
                write!(f, "species \"{}\" was not found on GBIF", species)
            }
            Error::InitializeCropperFailed => write!(f, "error initializing cropper"),
            Error::RocketError(e) => write!(f, "error with rocket: {}", e),
            Error::InternalServerError => write!(f, "internal server error"),
        }
    }
}

/// The result type of this library.
pub type Result<T> = StdResult<T, Error>;

impl From<reqwest::Error> for Error {
    fn from(error: reqwest::Error) -> Error {
        Error::ReqwestError(error)
    }
}

impl From<io::Error> for Error {
    fn from(error: io::Error) -> Error {
        Error::IoError(error)
    }
}

impl From<TpError> for Error {
    fn from(error: TpError) -> Error {
        Error::PostgresError(error)
    }
}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Error {
        Error::JsonError(error)
    }
}

impl From<rocket::Error> for Error {
    fn from(error: rocket::Error) -> Error {
        Error::RocketError(error)
    }
}

/// A wrapper for a database connection extrated from a pool.
pub struct Db(Object<ergol::pool::Manager>);

impl Db {
    /// Extracts a database from a pool.
    pub async fn from_pool(pool: Pool) -> Result<Db> {
        Ok(Db(pool.get().await.map_err(|_| Error::DbError)?))
    }
}

impl std::ops::Deref for Db {
    type Target = Object<ergol::pool::Manager>;
    fn deref(&self) -> &Self::Target {
        &*&self.0
    }
}

impl std::ops::DerefMut for Db {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut *&mut self.0
    }
}

impl ergol::Queryable<ergol::tokio_postgres::Client> for Db {
    fn client(&self) -> &ergol::tokio_postgres::Client {
        &std::ops::Deref::deref(&self.0).client
    }
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for Db {
    type Error = Error;

    async fn from_request(request: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let pool = match request.guard::<&State<Pool>>().await {
            Outcome::Success(pool) => pool,
            Outcome::Error(_) => {
                return Outcome::Error((Status::InternalServerError, Error::InternalServerError))
            }
            Outcome::Forward(s) => return Outcome::Forward(s),
        };

        let db = match pool.get().await {
            Ok(db) => db,
            Err(_) => {
                return Outcome::Error((Status::InternalServerError, Error::InternalServerError))
            }
        };

        Outcome::Success(Db(db))
    }
}

/// Scraps occurrences and then medias.
pub async fn scrap(
    taxon: Taxon,
    query: &str,
    min_occurrences: usize,
    max_occurrences: usize,
    crop: bool,
    config: &Config,
) -> Result<()> {
    let pool =
        ergol::pool(&config.databases.database.url, 32).expect("Failed to connect to the database");

    let mut db = Db::from_pool(pool.clone())
        .await
        .expect("Failed to connect to the database");

    // Create occurrences directory
    let species_dir = config.storage.species_dir();
    let species_dir = species_dir.to_str().expect("Failed to convert path to str");
    create_dir_all(species_dir).expect(&format!(
        "Failed to create species directory \"{}\"",
        species_dir
    ));

    // Find species matching query
    let species = Entry::from_taxon(taxon, query)?;
    let species_len = species.len();

    // Start by scraping species and occurrences
    for (index, species) in species.into_iter().enumerate() {
        info!(
            "{:05.2}% [1/2] [{:05}/{}] {}",
            100.0 * (index as f32 + 1.0) / species_len as f32,
            index + 1,
            species_len,
            species.valid_name
        );

        let transaction = db.transaction().await?;

        let s = Species::scrap_occurrences(
            species,
            max_occurrences,
            &BLACKLISTED_DATASET,
            &config.storage,
            &transaction,
        )
        .await;

        transaction.commit().await?;

        match s {
            Ok(s) => {
                if let Some(species_key) = s.species_key {
                    let medias_dir = config.storage.medias_dir(species_key);
                    let medias_dir = medias_dir.to_str().expect("Failed to convert path to str");
                    create_dir_all(medias_dir).expect(&format!(
                        "Failed to create species directory \"{}\"",
                        medias_dir
                    ));
                }
            }
            Err(e) => {
                error!("{}", e);
            }
        }
    }

    info!("Marking medias to download");

    // We're doing this with two big requests
    // First one: mark every first media for every occurrence
    info!("First request: first media for each occurrence");
    let sql = r#"
        UPDATE medias
        SET to_download = TRUE
        FROM (
            SELECT DISTINCT ON (occurrence) medias.id
            FROM medias, occurrences
            WHERE
                medias.occurrence = occurrences.id and
                occurrences.dataset_key != $1
            ORDER BY
                medias.occurrence, medias.id
        ) AS subquery
        WHERE medias.id = subquery.id;
    "#;

    info!("{}", sql);
    db.client().query(sql, &[&BLACKLISTED_DATASET]).await?;

    // Second one: mark every media for every species with available_occurences < min_occurrences
    // This request does not take into account the available_occurences attribute which counts
    // blacklisted datasets.
    info!(
        "Second request: every media for each species with less than {} occurrences",
        min_occurrences
    );

    let sql = r#"
        UPDATE medias
        SET to_download = TRUE
        FROM (
            SELECT occurrences.id
            FROM occurrences,
                (
                    SELECT occurrences.species
                    FROM occurrences
                    WHERE occurrences.dataset_key != $1
                    GROUP BY occurrences.species
                    HAVING count(occurrences.id) < $2
                ) as subquery
            WHERE occurrences.species = subquery.species
        ) AS subquery2
        WHERE medias.id = subquery2.id;
    "#;

    info!("{}", sql);
    db.client()
        .query(sql, &[&BLACKLISTED_DATASET, &(min_occurrences as i64)])
        .await?;

    // First pass: download all media marked to_download
    let cropper = if crop {
        info!("initializing cropper");
        let db = Db::from_pool(pool.clone())
            .await
            .expect("Failed to connect to the database");

        let (tx, rx) = unbounded_channel();
        let cropper =
            Cropper::new(config.batch_size, config.clone(), db).expect("Failed to create cropper");

        Some((cropper.run(rx), tx))
    } else {
        None
    };

    info!("Scrap medias");
    let client = reqwest::ClientBuilder::new()
        .timeout(Duration::from_secs(30))
        .build()
        .unwrap();

    // Count medias to download for showing progress
    let total: i64 = db
        .client()
        .query_one("SELECT count(id) FROM medias WHERE to_download;", &[])
        .await?
        .get(0);

    let mut count = 0;

    let mut offset = 0;
    let chunk_size = 100000;

    let semaphore = Semaphore::new(config.jobs);

    let mut handles = vec![];

    loop {
        let medias = Media::select()
            .filter(db::media::to_download::eq(true))
            .order_by(db::media::id::ascend())
            .offset(offset)
            .limit(chunk_size)
            .execute(&mut db.transaction().await?)
            .await?;

        let len = medias.len();

        for media in medias {
            let pool = pool.clone();
            let client = client.clone();
            let config = config.clone();
            let sender = cropper.as_ref().map(|x| x.1.clone());

            let _ = semaphore.acquire().await.unwrap();

            // Remove finished handles
            handles.retain(|x: &JoinHandle<_>| !x.is_finished());

            count += 1;

            handles.push(tokio::spawn(async move {
                let mut media = media;
                let db = Db::from_pool(pool).await.unwrap();
                let result = media.download(&client, &config.storage, &db).await;

                match result {
                    Ok(c) if c == 299 => info!(
                        "[2/2] {:05.2}% [{:05}/{}] Skipped download {} {}",
                        100.0 * count as f32 / total as f32,
                        count,
                        total,
                        media.id,
                        media.url
                    ),
                    Ok(c) if 200 <= c && c < 400 => {
                        // Ask cropper to crop media if necessary
                        if let Some(sender) = sender {
                            sender.send(Some(media.id)).unwrap();
                        }

                        info!(
                            "[2/2] {:05.2}% [{:05}/{}] Successfully downloaded {} {}",
                            100.0 * count as f32 / total as f32,
                            count,
                            total,
                            media.id,
                            media.url
                        );
                    }
                    Ok(e) => error!(
                        "[2/2] {:05.2}% [{:05}/{}] Failed downloading {} {} {}",
                        100.0 * count as f32 / total as f32,
                        count,
                        total,
                        media.id,
                        media.url,
                        e
                    ),
                    Err(e) => error!(
                        "[2/2] {:05.2}% [{:05}/{}] Failed downloading {} {} {}",
                        100.0 * count as f32 / total as f32,
                        count,
                        total,
                        media.id,
                        media.url,
                        e
                    ),
                }
            }));
        }

        if len < chunk_size {
            break;
        }

        offset += chunk_size;
    }

    for handle in handles {
        handle.await.unwrap();
    }

    // Finalize cropper
    if let Some((handle, sender)) = cropper {
        sender.send(None).unwrap();
        handle.await.unwrap();
    }

    info!("Scraping finished");

    Ok(())
}

async fn crop(batch_size: usize, config: &Config) -> Result<()> {
    let pool =
        ergol::pool(&config.databases.database.url, 32).expect("Failed to connect to the database");

    let db = Db::from_pool(pool.clone())
        .await
        .expect("Failed to connect to the database");

    let db_clone = Db::from_pool(pool.clone())
        .await
        .expect("Failed to connect to the database");

    let mut cropper =
        Cropper::new(batch_size, config.clone(), db_clone).expect("Failed to create cropper");

    let mut offset = 0;
    let chunk_size = 100000;

    loop {
        use db::media;
        let medias = Media::select()
            .filter(media::to_download::eq(true).and(media::cropped::eq(false)))
            .order_by(db::media::id::ascend())
            .offset(offset)
            .limit(chunk_size)
            .execute(&db)
            .await?;

        let len = medias.len();

        for media in medias {
            if media.path.is_some() {
                cropper.add_media(&media).await?;
            }
        }

        if len < chunk_size {
            break;
        }

        offset += chunk_size;
    }

    // Finalize cropper
    cropper.end().await?;

    Ok(())
}

/// Prints a pretty help.
pub fn print_help() {
    println!("NO HELP FOR YOU");
}

fn print_version() {
    println!("scraper {}", env!("CARGO_PKG_VERSION"));
}

/// Main.
pub async fn main() -> Result<()> {
    let args = args().collect::<Vec<_>>();

    // The first argument is the name of the binary, the second one is the command
    if args.len() < 2 {
        print_help();
        exit(1);
    }

    if args.contains(&String::from("-h")) || args.contains(&String::from("--help")) {
        print_help();
        exit(0);
    }

    if args.contains(&String::from("-v")) || args.contains(&String::from("--version")) {
        print_version();
        exit(0);
    }

    let config = Config::from_figment(&rocket::Config::figment());

    let log_dir = config.storage.data_path.join("logs");
    create_dir_all(&log_dir).expect("Failed to create log directory");

    let module = vec![String::from(module_path!())];
    let filename = format!("{}", Local::now().format("%Y-%m-%d--%H-%M-%S.log"));
    let logfile = File::create(log_dir.join(filename)).expect("Failed to create log file");
    Log::init(logfile, module).expect("Failed to init logging system");

    match args[1].as_ref() {
        "reset-db" => ergol_cli::reset(".").await.expect("Failed to reset db"),

        "scrap" => {
            if args.len() < 3 {
                print_help();
                exit(1);
            }

            let query = args[2].split("=").collect::<Vec<_>>();

            if query.len() != 2 {
                print_help();
                exit(1);
            }

            let taxon = match query[0].parse::<Taxon>() {
                Ok(taxon) => taxon,
                Err(e) => {
                    error!("{}", e);
                    exit(1);
                }
            };

            scrap(taxon, query[1], 30, 1200, true, &config).await?;
        }

        "crop" => {
            crop(128, &config).await?;
        }

        "serve" => {
            if let Err(e) = server::serve().await {
                error!("{}", e);
                return Err(e.into());
            }
        }

        command => {
            // Unknwon command
            error!(
                "{}: {}{}{}",
                "error", // .bold().red(),
                "command \"",
                command,
                "\" does not exist."
            );
            print_help();
            exit(1);
        }
    }

    Ok(())
}

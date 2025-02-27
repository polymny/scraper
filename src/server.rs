//! This module contains all the routes and utils for the webserver.

use std::path::{Path, PathBuf};
use std::result::Result as StdResult;

use serde_json::json;

use ergol::prelude::*;

use rocket::fairing::AdHoc;
use rocket::fs::NamedFile;
use rocket::{self, Ignite, Rocket};

use rocket_dyn_templates::{context, Template};

use crate::config::{Config, BLACKLISTED_DATASET};
use crate::db::Media;
use crate::db::Species;
use crate::logger::LogFairing;
use crate::utils::{pretty_finder, pretty_name};
use crate::Db;

/// Index route of our application.
#[get("/")]
pub async fn index(db: Db) -> Template {
    // Beautiful sql request
    let sql = r#"
    SELECT
        subquery.species_id,
        subquery.species_key,
        subquery.valid_name,
        subquery.media_path,
        COUNT(DISTINCT occurrences.id),
        COUNT(medias.id)
    FROM (
        SELECT
            DISTINCT ON (speciess.id)
            speciess.id AS species_id,
            speciess.species_key AS species_key,
            speciess.valid_name AS valid_name,
            medias.path AS media_path
        FROM
            speciess, occurrences, medias
        WHERE
            speciess.id = occurrences.species AND
            occurrences.id = medias.occurrence AND
            occurrences.dataset_key != $1 AND
            200 <= medias.status_code AND medias.status_code < 400 AND
            medias.path IS NOT NULL
        ) AS subquery, occurrences, medias
    WHERE
        subquery.species_id = occurrences.species AND
        occurrences.id = medias.occurrence AND
        occurrences.dataset_key != $1 AND
        200 <= medias.status_code AND medias.status_code < 400 AND
        medias.path IS NOT NULL
    GROUP BY
        subquery.species_id, subquery.valid_name, subquery.species_key, subquery.media_path
    ;
    "#;

    let species = db
        .client()
        .query(sql, &[&BLACKLISTED_DATASET])
        .await
        .unwrap()
        .into_iter()
        .map(|x| {
            let species_key = x.get::<usize, i64>(1);
            let valid_name = x.get::<usize, String>(2);
            let media_path = x.get::<usize, String>(3);
            let occurrence_count = x.get::<usize, i64>(4);
            let media_count = x.get::<usize, i64>(5);

            json!({
                "species_key": species_key,
                "valid_name": valid_name,
                "pretty_name": pretty_name(&valid_name),
                "pretty_finder": pretty_finder(&valid_name),
                "media_path": media_path,
                "occurrence_count": occurrence_count,
                "media_count": media_count,
            })
        })
        .collect::<Vec<_>>();

    Template::render("index", context! { species: species })
}

/// Route for visualising a media.
#[get("/<species_key>/<occurrence_key>/<media_index>")]
pub async fn media(species_key: i64, occurrence_key: i64, media_index: i32, db: Db) -> Template {
    let species = Species::get_by_species_key(species_key, &db)
        .await
        .unwrap()
        .unwrap();

    let media = Media::get_by_id(media_index, &db).await.unwrap().unwrap();

    Template::render(
        "media",
        context! {
            species_pretty_name: pretty_name(&species.valid_name),
            species_pretty_finder: pretty_finder(&species.valid_name),
            species_key: species_key,
            occurrence_key: occurrence_key,
            media_id: format!("{:04}", media.id),
            media: media,
        },
    )
}

/// Route for static files.
#[get("/static/<file..>")]
async fn static_files(file: PathBuf) -> Option<NamedFile> {
    NamedFile::open(Path::new("static/").join(file)).await.ok()
}

/// Route for scraped data.
#[get("/data/<file..>")]
async fn data_files(file: PathBuf) -> Option<NamedFile> {
    NamedFile::open(Path::new("data/").join(file)).await.ok()
}

/// Starts the web server.
pub async fn serve() -> StdResult<Rocket<Ignite>, rocket::Error> {
    rocket::build()
        .attach(Template::fairing())
        .attach(AdHoc::on_ignite("Config", |rocket| async move {
            let config = Config::from_rocket(&rocket);
            rocket.manage(config)
        }))
        .attach(AdHoc::on_ignite("Database", |rocket| async move {
            let config = Config::from_rocket(&rocket);
            let pool = ergol::pool(&config.databases.database.url, 32).unwrap();
            rocket.manage(pool)
        }))
        .attach(LogFairing)
        .mount("/", routes![index, media, static_files, data_files,])
        .ignite()
        .await?
        .launch()
        .await
}

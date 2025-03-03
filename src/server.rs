//! This module contains all the routes and utils for the webserver.

use std::path::{Path, PathBuf};
use std::process::exit;
use std::result::Result as StdResult;

use serde_json::{json, Value};

use ergol::prelude::*;

use tera::{Context, Tera};

use rocket::fairing::AdHoc;
use rocket::fs::NamedFile;
use rocket::response::content::RawHtml;
use rocket::{self, Ignite, Rocket, State as S};

use crate::config::{Config, BLACKLISTED_DATASET};
use crate::db::Media;
use crate::db::Species;
use crate::logger::LogFairing;
use crate::utils::{pretty_finder, pretty_name};
use crate::{Db, Result};

/// Number of items per page.
const LIMIT: i64 = 16;

/// Easily return `RawHtml<String>`.
type Html = RawHtml<String>;

/// Helper trait to be able to easily render json.
trait Renderable {
    /// Uses tera to render with json data as context.
    fn render_json(&self, template_name: &str, value: Value) -> Result<Html>;
}

impl Renderable for Tera {
    fn render_json(&self, template_name: &str, value: Value) -> Result<Html> {
        let context = Context::from_serialize(value)?;
        Ok(RawHtml(self.render(template_name, &context)?))
    }
}

/// Returns the number of species.
pub async fn count_species(db: &Db) -> Result<i64> {
    // Count species to know page number
    let sql = r#"
        SELECT
            COUNT(DISTINCT speciess.id)
        FROM
            speciess, occurrences, medias
        WHERE
            speciess.id = occurrences.species AND
            occurrences.id = medias.occurrence AND
            occurrences.dataset_key != $1 AND
            200 <= medias.status_code AND medias.status_code < 400 AND
            medias.path IS NOT NULL
        ;
    "#;

    let species_count = db
        .client()
        .query(sql, &[&BLACKLISTED_DATASET])
        .await?
        .into_iter()
        .next()
        .unwrap()
        .get::<usize, i64>(0);

    Ok(species_count)
}

/// Index route.
#[get("/")]
pub fn index(tera: &S<Tera>) -> Result<Html> {
    tera.render_json("index.html", json!({}))
}

/// List the species.
#[get("/species/list/<page>")]
pub async fn species(tera: &S<Tera>, db: Db, page: Option<u32>) -> Result<Html> {
    let page = page.unwrap_or(1);

    let species_count = count_species(&db).await?;

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
            speciess.reign AS reign,
            speciess.phylum AS phylum,
            speciess.class AS class,
            speciess.order AS order,
            speciess.genus AS genus,
            speciess.valid_name AS valid_name,
            speciess.species_key AS species_key,
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
        subquery.species_id,
        subquery.reign,
        subquery.phylum,
        subquery.class,
        subquery.order,
        subquery.genus,
        subquery.valid_name,
        subquery.species_key,
        subquery.media_path
    ORDER BY
        subquery.reign,
        subquery.phylum,
        subquery.class,
        subquery.order,
        subquery.genus,
        subquery.valid_name
    OFFSET
        $2
    LIMIT
        $3
    ;
    "#;

    let offset = (page - 1) as i64 * LIMIT;
    let species = db
        .client()
        .query(sql, &[&BLACKLISTED_DATASET, &offset, &LIMIT])
        .await?
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

    let max_page = species_count / LIMIT + 1;

    tera.render_json(
        "species.html",
        json!({
            "current_page": page,
            "species": species,
            "species_count": species_count,
            "max_page": max_page,
            "offset": offset,
            "limit": LIMIT,
        }),
    )
}

/// Route for visualizing medias for a certain species.
#[get("/species/key/<species_key>")]
pub async fn species_by_key(species_key: i64, tera: &S<Tera>, db: Db) -> Result<Html> {
    let species = Species::get_by_species_key(species_key, &db)
        .await?
        .unwrap();

    let sql = r#"
        SELECT
            medias.*
        FROM
            speciess,
            occurrences,
            medias
        WHERE
            speciess.species_key = $1 AND
            occurrences.species = speciess.id AND
            medias.occurrence = occurrences.id AND
            200 <= medias.status_code AND medias.status_code < 400
        ;
    "#;

    let medias = db.client().query(sql, &[&species_key]).await?;
    let medias = medias.iter().map(Media::from_row).collect::<Vec<_>>();

    tera.render_json(
        "species-key.html",
        json!({
            "species": species.to_json(&db).await?,
            "medias": medias,
        }),
    )
}

/// Route for visualising a media.
#[get("/media/<species_key>/<occurrence_key>/<media_index>")]
pub async fn media(
    species_key: i64,
    occurrence_key: i64,
    media_index: i32,
    tera: &S<Tera>,
    db: Db,
) -> Result<Html> {
    let species = Species::get_by_species_key(species_key, &db)
        .await?
        .unwrap();

    let media = Media::get_by_id(media_index, &db).await?.unwrap();

    tera.render_json(
        "media.html",
        json!({
            "species_pretty_name": pretty_name(&species.valid_name),
            "species_pretty_finder": pretty_finder(&species.valid_name),
            "species_key": species_key,
            "occurrence_key": occurrence_key,
            "media_id": format!("{:04}", media.id),
            "media": media,
        }),
    )
}

/// Route for static files.
#[get("/static/<file..>")]
async fn static_files(file: PathBuf) -> Option<NamedFile> {
    NamedFile::open(Path::new("static/").join(file)).await.ok()
}

/// Route for scraped data.
#[get("/data/<file..>")]
async fn data_files(config: &S<Config>, file: PathBuf) -> Option<NamedFile> {
    NamedFile::open(config.storage.data_path.join(file))
        .await
        .ok()
}

/// Starts the web server.
pub async fn serve() -> StdResult<Rocket<Ignite>, rocket::Error> {
    rocket::build()
        .attach(AdHoc::on_ignite("Config", |rocket| async move {
            let config = Config::from_rocket(&rocket);
            rocket.manage(config)
        }))
        .attach(AdHoc::on_ignite("Database", |rocket| async move {
            let config = Config::from_rocket(&rocket);
            let pool = ergol::pool(&config.databases.database.url, 32).unwrap();
            rocket.manage(pool)
        }))
        .attach(AdHoc::on_ignite("Tera", |rocket| async move {
            let mut tera = match Tera::new("templates/**/*.html") {
                Ok(t) => t,
                Err(e) => {
                    error!("while parsing tera templates: {}", e);
                    exit(1);
                }
            };
            tera.autoescape_on(vec![".html"]);
            rocket.manage(tera)
        }))
        .attach(LogFairing)
        .mount(
            "/",
            routes![
                index,
                species,
                species_by_key,
                media,
                static_files,
                data_files,
            ],
        )
        .ignite()
        .await?
        .launch()
        .await
}

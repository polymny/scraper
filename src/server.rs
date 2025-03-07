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
use crate::taxref::Taxon;
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

/// Index route.
#[get("/")]
pub fn index(tera: &S<Tera>) -> Result<Html> {
    tera.render_json("index.html", json!({}))
}

/// List the species.
#[get("/species/<taxon_key>/<taxon_value>/<page>")]
pub async fn species(
    taxon_key: Taxon,
    taxon_value: &str,
    page: u32,
    tera: &S<Tera>,
    db: Db,
) -> Result<Html> {
    if let Taxon::Species = taxon_key {
        species_by_valid_name(taxon_value, page, tera, db).await
    } else {
        species_list(taxon_key, taxon_value, page, tera, db).await
    }
}

/// Returns the HTML page that List species with a specific taxon filter.
pub async fn species_list(
    taxon: Taxon,
    taxon_value: &str,
    page: u32,
    tera: &S<Tera>,
    db: Db,
) -> Result<Html> {
    let taxon_key = if let Taxon::Species = taxon {
        "valid_name"
    } else {
        taxon.to_str()
    };

    // Count species to know page number

    // Because taxon_key comes from type Taxon, we can safely format it into the SQL query without
    // fearing SQL injection: anything that doesn't parse to taxon will fail before this route.
    let sql = format!(
        r#"
        SELECT
            COUNT(DISTINCT speciess.id)
        FROM
            speciess, occurrences, medias
        WHERE
            speciess.id = occurrences.species AND
            occurrences.id = medias.occurrence AND
            occurrences.dataset_key != $1 AND
            200 <= medias.status_code AND medias.status_code < 400 AND
            medias.path IS NOT NULL AND
            speciess.{} = $2
        ;
    "#,
        taxon_key
    );

    let species_count = db
        .client()
        .query(&sql, &[&BLACKLISTED_DATASET, &taxon_value])
        .await?
        .into_iter()
        .next()
        .unwrap()
        .get::<usize, i64>(0);

    // Beautiful sql request
    let sql = format!(
        r#"
    SELECT
        subquery.species_id,
        subquery.species_key,
        subquery.reign,
        subquery.phylum,
        subquery.class,
        subquery.order,
        subquery.family,
        subquery.genus,
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
            speciess.family AS family,
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
            medias.path IS NOT NULL AND
            speciess.{} = $2
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
        subquery.family,
        subquery.genus,
        subquery.valid_name,
        subquery.species_key,
        subquery.media_path
    ORDER BY
        subquery.reign,
        subquery.phylum,
        subquery.class,
        subquery.order,
        subquery.family,
        subquery.genus,
        subquery.valid_name
    OFFSET
        $3
    LIMIT
        $4
    ;
    "#,
        taxon_key
    );

    let mut breadcrumb = None;
    let offset = (page - 1) as i64 * LIMIT;
    let species = db
        .client()
        .query(&sql, &[&BLACKLISTED_DATASET, &taxon_value, &offset, &LIMIT])
        .await?
        .into_iter()
        .map(|x| {
            let species_key = x.get::<usize, i64>(1);
            let valid_name = x.get::<usize, String>(8);
            let media_path = x.get::<usize, String>(9);
            let occurrence_count = x.get::<usize, i64>(10);
            let media_count = x.get::<usize, i64>(10);

            if breadcrumb.is_none() {
                breadcrumb = Some(vec![
                    x.get::<usize, String>(2),
                    x.get::<usize, String>(3),
                    x.get::<usize, String>(4),
                    x.get::<usize, String>(5),
                    x.get::<usize, String>(6),
                    x.get::<usize, String>(7),
                    x.get::<usize, String>(8),
                ]);
            }

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

    let breadcrumb = breadcrumb.unwrap_or(vec![]);

    // Keep only the level of the research
    let breadcrumb = match taxon {
        Taxon::Reign => &breadcrumb[0..1],
        Taxon::Phylum => &breadcrumb[0..2],
        Taxon::Class => &breadcrumb[0..3],
        Taxon::Order => &breadcrumb[0..4],
        Taxon::Family => &breadcrumb[0..5],
        Taxon::Genus => &breadcrumb[0..6],
        Taxon::Species => &breadcrumb[0..7],
    };

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
            "breadcrumb": breadcrumb,
            "breadcrumb_len": breadcrumb.len(),
        }),
    )
}

/// Route for visualizing medias for a certain species.
pub async fn species_by_valid_name(
    valid_name: &str,
    page: u32,
    tera: &S<Tera>,
    db: Db,
) -> Result<Html> {
    let species = Species::get_by_valid_name(valid_name, &db).await?.unwrap();

    let sql = r#"
        SELECT
            medias.*
        FROM
            speciess,
            occurrences,
            medias
        WHERE
            speciess.valid_name = $1 AND
            occurrences.species = speciess.id AND
            medias.occurrence = occurrences.id AND
            occurrences.dataset_key != $2 AND
            200 <= medias.status_code AND medias.status_code < 400
        ORDER BY
            medias.id
        ;
    "#;

    let medias = db
        .client()
        .query(sql, &[&valid_name, &BLACKLISTED_DATASET])
        .await?;
    let medias_len = medias.len();
    let offset = (page - 1) as i64 * LIMIT;
    let medias = medias.iter().map(Media::from_row).collect::<Vec<_>>();
    let medias_cropped_len = medias.iter().filter(|x| x.x.is_some()).count();
    let max_page = (medias.len() / LIMIT as usize) + 1;

    tera.render_json(
        "species-key.html",
        json!({
            "species": species.to_json(&db).await?,
            "medias_len": medias_len,
            "medias_cropped_len": medias_cropped_len,
            "medias": medias[offset as usize .. std::cmp::min((offset + LIMIT) as usize, medias.len())],
            "current_page": page,
            "max_page": max_page,
            "offset": offset,
            "limit": LIMIT,
        }),
    )
}

/// Test route for plotly.
#[get("/plotly")]
pub async fn plotly(tera: &S<Tera>, db: Db) -> Result<Html> {
    // Beautiful sql request
    let sql = r#"
        SELECT
            subquery.species_id,
            subquery.species_key,
            subquery.reign,
            subquery.phylum,
            subquery.class,
            subquery.order,
            subquery.family,
            subquery.genus,
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
                speciess.family AS family,
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
            subquery.family,
            subquery.genus,
            subquery.valid_name,
            subquery.species_key,
            subquery.media_path
        ORDER BY
            subquery.reign,
            subquery.phylum,
            subquery.class,
            subquery.order,
            subquery.family,
            subquery.genus,
            subquery.valid_name
        ;
    "#;

    let species = db
        .client()
        .query(sql, &[&BLACKLISTED_DATASET])
        .await?
        .into_iter()
        .map(|x| {
            let species_key = x.get::<usize, i64>(1);
            let reign = x.get::<usize, String>(2);
            let phylum = x.get::<usize, String>(3);
            let class = x.get::<usize, String>(4);
            let order = x.get::<usize, String>(5);
            let family = x.get::<usize, String>(6);
            let genus = x.get::<usize, String>(7);
            let valid_name = x.get::<usize, String>(8);
            let media_path = x.get::<usize, String>(9);
            let occurrence_count = x.get::<usize, i64>(10);
            let media_count = x.get::<usize, i64>(10);

            json!({
                "species_key": species_key,
                "reign": reign,
                "phylum": phylum,
                "class": class,
                "order": order,
                "family": family,
                "genus": genus,
                "valid_name": valid_name,
                "pretty_name": pretty_name(&valid_name),
                "pretty_finder": pretty_finder(&valid_name),
                "media_path": media_path,
                "occurrence_count": occurrence_count,
                "media_count": media_count,
            })
        })
        .collect::<Vec<_>>();

    let mut genuses = vec![];
    let mut previous_genus = None;

    for species in &species {
        let current_genus = species.get("genus");
        if previous_genus != Some(current_genus) {
            genuses.push((current_genus, species.get("family")));
            previous_genus = Some(current_genus);
        }
    }

    tera.render_json(
        "plotly.html",
        json!({
            "species": species,
            "genuses": genuses,
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
            routes![index, species, plotly, media, static_files, data_files,],
        )
        .ignite()
        .await?
        .launch()
        .await
}

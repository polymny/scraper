//! This module contains all the routes and utils for the webserver.

use std::path::{Path, PathBuf};
use std::result::Result as StdResult;

use rocket::fs::NamedFile;
use rocket::{self, Ignite, Rocket};

use rocket_dyn_templates::{context, Template};

use crate::config::Config;
use crate::logger::LogFairing;

/// Index route of our application.
#[get("/")]
pub fn index() -> Template {
    Template::render("index", context! {})
}

/// Route for static files.
#[get("/static/<file..>")]
async fn static_files(file: PathBuf) -> Option<NamedFile> {
    NamedFile::open(Path::new("static/").join(file)).await.ok()
}

/// Starts the web server.
pub async fn serve() -> StdResult<Rocket<Ignite>, rocket::Error> {
    let figment = rocket::Config::figment();
    let config = Config::from_figment(&figment);

    rocket::build()
        .attach(Template::fairing())
        .attach(LogFairing)
        .mount("/", routes![index, static_files])
        .ignite()
        .await?
        .launch()
        .await
}

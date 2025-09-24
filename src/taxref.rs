//! Module that helps us deal with taxref.

use std::fmt;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::result::Result as StdResult;
use std::str::FromStr;

use rocket::request::FromParam;

use crate::db::SpeciesTrait;
use crate::utils;
use crate::{Error, Result};

/// Retrieves the path of the taxref file on the local disk.
pub fn path() -> Option<PathBuf> {
    let mut target = dirs::cache_dir()?;
    target.push("gbif-scraper");
    target.push("TAXREFv17.txt");
    Some(target)
}

/// Downloads taxref in the cache directory.
///
/// Returns the path to the taxref file.
pub async fn download() -> Result<()> {
    let target = path().ok_or(Error::NoCache)?;

    // No need to download if it already exists
    if target.exists() {
        return Ok(());
    }

    utils::download("https://storage.tforgione.fr/TAXREFv17.txt", target).await?;

    Ok(())
}

/// The different taxonomic levels.
#[derive(Copy, Clone)]
pub enum Taxon {
    /// Reign.
    Reign,

    /// Phylum.
    Phylum,

    /// Class.
    Class,

    /// Order.
    Order,

    /// Family.
    Family,

    /// Genus (sub-family).
    Genus,

    /// Species.
    Species,
}

impl Taxon {
    /// Returns a string version of the taxon.
    pub fn to_str(self) -> &'static str {
        match self {
            Taxon::Reign => "reign",
            Taxon::Phylum => "phylum",
            Taxon::Class => "class",
            Taxon::Order => "order",
            Taxon::Family => "family",
            Taxon::Genus => "genus",
            Taxon::Species => "species",
        }
    }
}

impl<'a> FromParam<'a> for Taxon {
    type Error = NoSuchTaxon;

    fn from_param(param: &'a str) -> StdResult<Taxon, Self::Error> {
        param.parse::<Taxon>()
    }
}

/// An error that occured while parsing a taxon.
#[derive(Debug)]
pub struct NoSuchTaxon(String);

impl fmt::Display for NoSuchTaxon {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "no such taxon \"{}\"", self.0)
    }
}

impl FromStr for Taxon {
    type Err = NoSuchTaxon;
    fn from_str(input: &str) -> StdResult<Taxon, NoSuchTaxon> {
        match input.to_lowercase().as_str() {
            "reign" => Ok(Taxon::Reign),
            "phylum" => Ok(Taxon::Phylum),
            "class" => Ok(Taxon::Class),
            "order" => Ok(Taxon::Order),
            "family" => Ok(Taxon::Family),
            "genus" => Ok(Taxon::Genus),
            "species" => Ok(Taxon::Species),
            _ => Err(NoSuchTaxon(input.to_owned())),
        }
    }
}

/// A taxref entry.
#[derive(Debug, Clone, PartialOrd, Ord, PartialEq, Eq)]
pub struct Entry {
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

    /// Rank of the species.
    pub rank: String,

    /// Name of the species.
    pub name: String,

    /// Full name of the species.
    pub full_name: String,

    /// Valid name of the species.
    pub valid_name: String,

    /// Habitat of the species.
    pub habitat: String,

    /// Whether the species is present in france or not.
    pub fr: String,
}

impl SpeciesTrait for Entry {
    fn valid_name(&self) -> &str {
        &self.valid_name
    }
}

impl Entry {
    /// Returns true if the entry corresponds to a species.
    pub fn is_species(&self) -> bool {
        self.rank == "ES"
    }

    /// Returns true if the entry is present in france.
    pub fn is_present_france(&self) -> bool {
        self.fr == "P"
            || self.fr == "E"
            || self.fr == "I"
            || self.fr == "S"
            || self.fr == "C"
            || self.fr == "J"
            || self.fr == "M"
            || self.fr == "B"
    }

    /// Returns true if the entry is terrestrial.
    pub fn is_terrestrial(&self) -> bool {
        self.habitat == "2"
            || self.habitat == "3"
            || self.habitat == "5"
            || self.habitat == "7"
            || self.habitat == "8"
    }

    /// Returns true if the entry must be considered.
    pub fn filter(&self) -> bool {
        self.is_species() && self.is_present_france() && self.is_terrestrial()
    }

    /// Returns the corresponding name of the taxonomic level given as argument.
    pub fn get_taxon(&self, level: Taxon) -> &str {
        match level {
            Taxon::Reign => &self.reign,
            Taxon::Phylum => &self.phylum,
            Taxon::Class => &self.class,
            Taxon::Order => &self.order,
            Taxon::Family => &self.family,
            Taxon::Genus => &self.genus,
            Taxon::Species => &self.valid_name,
        }
    }

    /// Recreates an entry from a line in taxref.
    pub fn from_line(line: &str) -> Result<Entry> {
        let split = line
            .split("\t")
            .map(|x| x.replace("\"", ""))
            .collect::<Vec<_>>();

        Ok(Entry {
            reign: split[0].to_string(),
            phylum: split[1].to_string(),
            class: split[2].to_string(),
            order: split[3].to_string(),
            family: split[4].to_string(),
            genus: split[5].to_string(),
            rank: split[14].to_string(),
            name: split[15].to_string(),
            full_name: split[17].to_string(),
            valid_name: split[19].to_string(),
            habitat: split[22].to_string(),
            fr: split[23].to_string(),
        })
    }

    /// Retrives all the species corresponding to a specific filter.
    pub fn from_taxon(taxon: Taxon, query: &str) -> Result<Vec<Entry>> {
        let mut entries: Vec<Entry> = vec![];

        let taxref = path().ok_or(Error::NoCache)?;
        let taxref = File::open(taxref)?;

        for line in BufReader::new(taxref).lines().skip(1) {
            let line = line?;
            if line.is_empty() {
                continue;
            }

            let entry = Entry::from_line(&line)?;

            if entry.filter() && entry.get_taxon(taxon).to_lowercase() == query.to_lowercase() {
                // Found match, check to avoid duplicates
                if let Some(previous) = entries.last() {
                    if previous.valid_name == entry.valid_name {
                        continue;
                    }
                }

                // Match that is not a duplicate
                entries.push(entry);
            }
        }

        Ok(entries)
    }
}

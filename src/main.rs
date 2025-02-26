use std::process::exit;

#[rocket::main]
async fn main() -> Result<(), rocket::Error> {
    if let Err(e) = scraper::main().await {
        eprintln!("error: {}", e);
        exit(1);
    }
    Ok(())
}

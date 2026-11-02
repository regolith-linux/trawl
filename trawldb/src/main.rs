use clap::Parser;
use std::error::Error;
use trawldb::{parser::CliArgs, Client};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = CliArgs::parse();
    let res = match Client::new().await {
        Ok(client) => client.run(&args).await,
        Err(e) => Err(e),
    };
    if let Err(e) = res {
        // Like xrdb: report the failure and exit non-zero
        eprintln!("trawldb: {e}");
        std::process::exit(1);
    }
    Ok(())
}

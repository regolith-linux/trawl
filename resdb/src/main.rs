use clap::Parser;
use std::error::Error;
use resdb::{parser::CliArgs, Client};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = CliArgs::parse();
    let client = Client::new().await?;
    let res = client.run(&args).await;
    if let Err(e) = res {
        eprintln!("resdb: {e}");
    }
    Ok(())
}

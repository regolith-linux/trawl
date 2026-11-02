use std::{process, error::Error, future::pending};
use clap::Parser;
use trawld::{ResourceManager, parser::CliArgs};


#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = match CliArgs::try_parse() {
        Ok(args) => args,
        // --help and --version aren't errors: print them and exit 0
        Err(e) if !e.use_stderr() => e.exit(),
        Err(e) => {
            eprintln!("Error parsing: {e}");
            process::exit(1);
        }
    };
    let mut manager = ResourceManager::from_args(&args);
    manager.init();
    let _conn = manager.run_server().await?;
    pending::<()>().await;
    Ok(())
}

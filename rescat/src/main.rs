use std::{error::Error, process};
use clap::{Parser, AppSettings};
use resdb::Client;

#[derive(Parser)]
#[clap(author, version, about, setting(AppSettings::ArgRequiredElseHelp))]
struct Args {

    /// resource name
    #[clap(value_parser, required=true, index=1)]
    name: String,

    /// default value for resource
    #[clap(value_parser, index=2)]
    default: Option<String>

}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let args = Args::parse();
    let client = Client::new().await.unwrap();
    let resource = client.proxy().get_resource(&args.name).await?;
    if resource == String::new() {
        if let Some(default_val) = &args.default {
            println!("{}", default_val);
        }
        else {
            process::exit(1);
        }
    }
    else {
        println!("{}", resource);
    }
    Ok(())
}

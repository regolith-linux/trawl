use std::error::Error;
use clap::Parser;
use trawlcat::rescat;

#[derive(Parser)]
#[clap(author, version, about, arg_required_else_help=true)]
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
    let resource = rescat(&args.name, args.default).await?;
    print!("{resource}");
    Ok(())
}

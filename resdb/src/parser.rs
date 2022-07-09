use clap::Parser;

/// CLI interface for config manager client (resdb)
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
#[clap(author, version, about )]
pub struct CliArgs {

    /// load resources from file
    #[clap(short, long, value_parser, conflicts_with ="filename")]
    pub load: Option<String>,

    #[clap(short, long, value_parser)]
    pub merge: Option<String>,

    /// preprocessor to use [/usr/bin/cpp]
    #[clap(long, short, value_parser, conflicts_with="nocpp")]
    pub cpp: Option<String>,

    /// do not use a preprocessor
    #[clap(short, long, action)]
    pub nocpp: bool,

    #[clap(value_parser)]
    pub filename: Option<String>,

    #[clap(short, long, value_parser)]
    pub edit: Option<String>,

    #[clap(short, long, value_parser, requires="edit")]
    pub backup: Option<String>,

    #[clap(short, long, value_parser, conflicts_with="query")]
    pub get: Option<String>,

    #[clap(short, long, min_values=0, max_values=1)]
    pub query: Option<Vec<String>>
}

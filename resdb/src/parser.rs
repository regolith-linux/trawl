use clap::{Parser, AppSettings};

#[derive(Parser, Debug, Clone, PartialEq, Eq)]
#[clap(author, version, about, setting(AppSettings::ArgRequiredElseHelp) )]
/// Config Manager Client for resmand
pub struct CliArgs {

    /// load resources from file
    #[clap(short, long, value_parser, conflicts_with ="filename", value_name="filename")]
    pub load: Option<String>,

    /// merge resources from file & sort
    #[clap(short, long, value_parser, value_name="filename")]
    pub merge: Option<String>,

    /// preprocessor to use [/usr/bin/cpp]
    #[clap(long, short, value_parser, conflicts_with="nocpp")]
    pub cpp: Option<String>,

    /// do not use a preprocessor
    #[clap(long, action)]
    pub nocpp: bool,

    /// File to load
    #[clap(value_parser)]
    pub filename: Option<String>,

    /// edit resources into file
    #[clap(short, long, value_parser, value_name="filename")]
    pub edit: Option<String>,

    /// backup suffix for -edit [.bak]
    #[clap(short, long, value_parser, requires="edit", value_name="string")]
    pub backup: Option<String>,

    /// get the content of a resource
    #[clap(short, long, value_parser, conflicts_with="query", value_name="name")]
    pub get: Option<String>,

    /// query resources
    #[clap(short, long, value_parser, min_values=0, max_values=1 ,value_name="string")]
    pub query: Option<Vec<String>>,
}

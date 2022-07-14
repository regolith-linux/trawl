use clap::Parser;

/// CLI interface for config manager daemon
#[derive(Parser, Debug, Clone, PartialEq, Eq)]
#[clap(author, version, about )]
pub struct CliArgs {

    /// load resources from file
    #[clap(short, long, value_parser, value_name = "filename")]
    pub load: Option<String>,

    /// preprocessor to use [/usr/bin/cpp]
    #[clap(long, value_parser, value_name = "filename")]
    pub cpp: Option<String>,

    /// do not use a preprocessor
    #[clap(long, action)]
    pub nocpp: bool,

    #[clap(value_parser)]
    pub filename: Option<String>,

    #[clap(long, short, action)]
    pub debug: bool,

    #[clap(long, short, action)]
    pub verbose: bool
}

use std::{panic::Location, error::Error};

use crate::parser::CliArgs;
use colored::Colorize;

pub enum LogMsg<'a> { 
    Info(&'a str),
    Warn(&'a str),
    Error(&'a str)
}

impl<'a> LogMsg<'a> {
    pub fn print(&self) {
        match self {
            Self::Info(x) => println!("[INFO]\t{x}"),
            Self::Warn(x) => eprintln!("{} {}", "[WARNING]".yellow(), x.yellow()),
            Self::Error(x) => eprintln!("{} {}", "[ERROR]".red(), x.red())
        }
    }
    pub fn print_with_loc(&self, loc: &Location) {
        match self {
            Self::Info(x) => println!("[INFO]\t{x}"),
            Self::Warn(x) => eprintln!("{} {}", "[WARNING]".yellow(), x.yellow()),
            Self::Error(x) => eprintln!("{} {} {}", loc, "[ERROR]".red(), x.red())
        }
    }
    pub fn is_info(&self) -> bool {
        match self {
            Self::Info(_) => true,
            _ => false,
        }
    }
    pub fn is_warning(&self) -> bool {
        match self {
            Self::Warn(_) => true,
            _ => false,
        }
    }
    pub fn is_error(&self) -> bool {
        match self {
            Self::Error(_) => true,
            _ => false,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct Logger {
    verbose: bool,
    debug: bool,
}


impl Logger {
    pub fn from(args: &CliArgs) -> Logger {
        let CliArgs{debug, verbose, ..} = *args;
        Logger { 
            verbose,
            debug,
        }
    } 
    pub fn log(&self, msg: LogMsg) {
        if self.debug || (self.verbose && !msg.is_info()) {
            msg.print();
        } 
    }
    pub fn warn(&self, msg: &str) {
        self.log(LogMsg::Warn(msg));
    }
    pub fn info(&self, msg: &str) {
        self.log(LogMsg::Info(msg));
    }
    #[track_caller]
    pub fn error(&self, msg: &str) {
        let caller_location = Location::caller();
        let err_obj = LogMsg::Error(msg);
        err_obj.print_with_loc(caller_location);
    }
    #[track_caller]
    pub fn from_error(&self, error: Box<dyn Error>) {
        let caller_location = Location::caller();
        let err_str = format!("{}", error);
        let err_obj = LogMsg::Error(&err_str);
        err_obj.print_with_loc(caller_location);
    }
}

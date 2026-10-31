mod common;
pub mod log;
pub mod parser;
pub mod tests;
mod xrm;

use common::trim_str;
use log::Logger;
use parser::CliArgs;
use std::{
    collections::HashMap,
    error::Error,
    fs::{self, File},
    path::Path,
    process::Command,
};
use zbus::{connection, interface, Connection, SignalContext};

/// Stores and manages the resources
#[derive(Debug, PartialEq, Eq)]
pub struct ResourceManager {
    resources: HashMap<String, String>,
    preprocessor: String,
    logger: Logger,
    args: CliArgs,
}

impl ResourceManager {
    /// Create new ResourceManager object from CliArgs
    pub fn from_args(args: &CliArgs) -> ResourceManager {
        let command = match &args.cpp {
            Some(cmd) => cmd.clone(),
            _ => String::from("/usr/bin/cpp"),
        };
        let resources = HashMap::new();
        let preprocessor = command;
        let logger = Logger::from(args);
        ResourceManager {
            resources,
            preprocessor,
            logger,
            args: args.clone(),
        }
    }

    /// Initialize ResourceManager fields based on the values in args
    pub fn init(&mut self) {
        self.logger.info("Initializing Daemon...");
        let filename = match &self.args.load {
            Some(file) => file,
            None => match &self.args.filename {
                Some(x) => x,
                None => return,
            },
        };
        self.load_from_file(
            &filename.clone(),
            self.args.nocpp,
            &self.preprocessor.clone(),
            &[],
        );
    }

    /// Runs the config daemon
    pub async fn run_server(self) -> zbus::Result<Connection> {
        connection::Builder::session()?
            .name("org.regolith.Trawl")?
            .serve_at("/org/regolith/Trawl", self)?
            .build()
            .await
    }

    /// Getter for preprocessor
    fn preprocessor(&self, cmd: &str) -> Command {
        Command::new(cmd)
    }

    /// Returns the content of the file after preprocessing
    fn get_preprocessed_file(
        &mut self,
        file_path: &str,
        nocpp: bool,
        cpp: &str,
        cpp_args: &[String],
    ) -> Result<String, Box<dyn Error>> {
        self.logger
            .info(&format!("{cpp} {cpp_args:?} < {file_path}"));
        if nocpp {
            self.logger
                .warn("wont use preprocessor - try running without --nocpp flag");
            let config_str = fs::read_to_string(file_path)?;
            self.logger.info("Config file read successfully");
            self.logger.info(&config_str);
            return Ok(config_str);
        }
        let output_bytes = self
            .preprocessor_command(cpp, cpp_args, file_path)?
            .output()?
            .stdout;

        let conf_utf8 = String::from_utf8(output_bytes)?;
        self.logger.info("File preprocessed successfully...");
        self.logger.info(&conf_utf8);
        Ok(conf_utf8)
    }

    /// Builds the preprocessor command for a resource file.
    ///
    /// The file is passed on stdin and never named on the command line, so
    /// the preprocessor can't take it for its output file, whatever the
    /// arguments are. The trailing "-" (stdin) keeps cpp's optional output
    /// operand from naming a file. The command runs in the file's directory
    /// so that `#include "..."` stays relative to the file.
    fn preprocessor_command(
        &self,
        cpp: &str,
        cpp_args: &[String],
        file_path: &str,
    ) -> Result<Command, Box<dyn Error>> {
        let input = File::open(file_path)?;
        let mut command = self.preprocessor(cpp);
        // Like xrdb: -P, no line markers, which would split continued lines
        command.arg("-P").args(cpp_args).arg("-").stdin(input);
        if let Some(dir) = Path::new(file_path).parent() {
            if !dir.as_os_str().is_empty() {
                command.current_dir(dir);
            }
        }
        Ok(command)
    }

    /// Checks if a given key is a valid resource specification and
    /// returns it normalized
    fn check_valid_key(&self, key: &str) -> Option<String> {
        let spec = xrm::normalize_spec(key);
        if spec.is_none() {
            self.logger.warn(&format!("{key} is not a valid key"));
        }
        spec
    }

    /// Parse the 'config_str' string into key value pairs. Like xrdb, the
    /// last entry for a specification wins.
    fn parse_config(&self, config_str: &str) -> HashMap<String, String> {
        let mut parsed_resources = HashMap::new();
        for (k, v) in xrm::parse(config_str) {
            // Skip line if key is not a valid resource specification
            if let Some(spec) = self.check_valid_key(&k) {
                merge_resource(&mut parsed_resources, spec, v);
            }
        }
        parsed_resources
    }

    /// Loads resources from the file. Doesn't override existing resources.
    fn load_from_file(&mut self, file: &str, nocpp: bool, cpp: &str, args: &[String]) {
        let config_str = match self.get_preprocessed_file(file, nocpp, cpp, args) {
            Ok(conf) => conf,
            Err(e) => {
                self.logger.from_error(e);
                return;
            }
        };
        let parsed_resources = self.parse_config(&config_str);
        self.logger
            .info(&format!("parsed_resources: {:#?}", parsed_resources));
        for (k, v) in parsed_resources {
            if xrm::find_spec(&self.resources, &k).is_none() {
                self.resources.insert(k, v);
            }
        }
        self.logger.info(&format!(
            "Updated resources after loading: \n {:#?}",
            &self.resources
        ))
    }

    /// Merges resources from the file with the loaded resources. Overrides
    /// value if key already presen in resources.
    fn merge_from_file(&mut self, file: &str, nocpp: bool, cpp: &str, args: &[String]) {
        let config_str = match self.get_preprocessed_file(file, nocpp, cpp, args) {
            Ok(conf) => conf,
            Err(e) => {
                self.logger.from_error(e);
                return;
            }
        };
        let parsed_resources = self.parse_config(&config_str);
        self.logger
            .info(&format!("parsed_resources: {:#?}", parsed_resources));
        for (k, v) in parsed_resources {
            merge_resource(&mut self.resources, k, v);
        }
        self.logger.info(&format!(
            "Updated resources after merging: \n {:#?}",
            &self.resources
        ))
    }

    /// Notify clients when resource changed
    pub async fn emit_resources_changed(&self, ctxt: &SignalContext<'_>) {
        if let Err(e) = self.resources_changed(&ctxt).await {
            self.logger.error(&format!("{e}"));
        }
    }

    /// removes all resources
    pub fn handle_remove_all(&mut self) {
        self.resources.clear();
    }

    /// removes single resource
    pub fn handle_remove_one(&mut self, key: &str) -> Option<(String, String)> {
        self.resources.remove_entry(key)
    }
}

/// Splits the argument string of the `LoadCpp` and `MergeCpp` methods.
fn split_legacy_args(args: &str) -> Vec<String> {
    args.split_whitespace().map(String::from).collect()
}

#[interface(name = "org.regolith.trawl1")]
impl ResourceManager {
    /// DBus Interface to load resources from file
    async fn load(
        &mut self,
        #[zbus(signal_context)] ctxt: SignalContext<'_>,
        path: &str,
        nocpp: bool,
    ) {
        self.load_from_file(path, nocpp, &self.preprocessor.clone(), &[]);
        self.emit_resources_changed(&ctxt).await;
    }

    /// DBus Interface to merge resources from file
    async fn merge(
        &mut self,
        #[zbus(signal_context)] ctxt: SignalContext<'_>,
        path: &str,
        nocpp: bool,
    ) {
        self.merge_from_file(path, nocpp, &self.preprocessor.clone(), &[]);
        self.emit_resources_changed(&ctxt).await;
    }

    /// DBus Interface to load resources from file using custom preprocessor.
    /// `args` is split on whitespace, so an argument can't contain a space;
    /// use `LoadCppArgv` instead.
    async fn load_cpp(
        &mut self,
        #[zbus(signal_context)] ctxt: SignalContext<'_>,
        path: &str,
        cpp: &str,
        args: &str,
    ) {
        self.load_from_file(path, false, cpp, &split_legacy_args(args));
        self.emit_resources_changed(&ctxt).await;
    }

    /// DBus Interface to merge resources from file using custom preprocessor.
    /// `args` is split on whitespace, so an argument can't contain a space;
    /// use `MergeCppArgv` instead.
    async fn merge_cpp(
        &mut self,
        #[zbus(signal_context)] ctxt: SignalContext<'_>,
        path: &str,
        cpp: &str,
        args: &str,
    ) {
        self.merge_from_file(path, false, cpp, &split_legacy_args(args));
        self.emit_resources_changed(&ctxt).await;
    }

    /// DBus Interface to load resources from file using custom preprocessor.
    /// Each element of `args` is passed to the preprocessor as one argument.
    async fn load_cpp_argv(
        &mut self,
        #[zbus(signal_context)] ctxt: SignalContext<'_>,
        path: &str,
        cpp: &str,
        args: Vec<String>,
    ) {
        self.load_from_file(path, false, cpp, &args);
        self.emit_resources_changed(&ctxt).await;
    }

    /// DBus Interface to merge resources from file using custom preprocessor.
    /// Each element of `args` is passed to the preprocessor as one argument.
    async fn merge_cpp_argv(
        &mut self,
        #[zbus(signal_context)] ctxt: SignalContext<'_>,
        path: &str,
        cpp: &str,
        args: Vec<String>,
    ) {
        self.merge_from_file(path, false, cpp, &args);
        self.emit_resources_changed(&ctxt).await;
    }

    /// Returns all the matching
    /// *Note*: Also a DBus interface
    pub fn query(&self, q: &str) -> String {
        let query_trimmed = trim_str(q);
        let mut matches: Vec<_> = self
            .resources
            .iter()
            .filter(|(k, _)| k.contains(query_trimmed))
            .collect();
        // Like `xrdb -query`: sorted, one line per entry, values escaped
        matches.sort();
        let query_result: String = matches
            .iter()
            .map(|(x, v)| format!("{}:\t{}\n", x, xrm::escape_value(v)))
            .collect();
        self.logger.info(&format!(
            "Following resources match the query '{query_trimmed}'\
                                  : {query_result}"
        ));
        query_result
    }

    /// Get the resource value, or "" if it is not found
    pub fn get_resource(&self, key: &str) -> String {
        self.lookup_resource(key).1
    }

    /// Look the resource up like Xlib, matching pattern entries. Returns
    /// whether it was found, and its value: unlike `get_resource`, tells a
    /// resource with an empty value from a missing one.
    /// *Note*: Also a DBus interface
    pub fn lookup_resource(&self, key: &str) -> (bool, String) {
        let key_trimmed = trim_str(key);
        let found = self
            .resources
            .get(key_trimmed)
            .or_else(|| xrm::lookup(&self.resources, key_trimmed));
        let value = found.cloned().unwrap_or_default();
        self.logger
            .info(&format!("value of key {key_trimmed} is {value}"));
        (found.is_some(), value)
    }

    /// DBus interface to set the value of a resource. Overwrites exiting value.
    /// TODO: Separate implementation to make more testable
    /// # Emits
    /// **resources_changed**: This signal indicated the change
    /// in'resources' property value
    pub async fn set_resource(
        &mut self,
        #[zbus(signal_context)] ctxt: SignalContext<'_>,
        key: String,
        val: String,
    ) {
        let key_normalized = normalize_key(&key);
        let val_trimmed = common::trim_str(&val);
        let curr_val = xrm::find_spec(&self.resources, &key_normalized).map(|k| &self.resources[k]);
        // Do not add key-value pair if key exists and current value is
        // same as the value to be inserted
        if let Some(x) = curr_val {
            if *x == val {
                return;
            }
        }
        merge_resource(
            &mut self.resources,
            key_normalized,
            String::from(val_trimmed),
        );
        self.emit_resources_changed(&ctxt).await;
    }

    /// DBus interface to add a new resource. Doesn't overwrite exiting
    /// values in case of conflicts
    /// TODO: Separate implementation to make more testable
    pub async fn add_resource(
        &mut self,
        #[zbus(signal_context)] ctxt: SignalContext<'_>,
        key: String,
        val: String,
    ) {
        let key_normalized = normalize_key(&key);
        let val_trimmed = trim_str(&val);
        let curr_val = xrm::find_spec(&self.resources, &key_normalized);
        // Do not add key value pair if key already defined
        if let Some(_) = curr_val {
            return;
        }
        self.resources
            .insert(key_normalized, String::from(val_trimmed));
        self.emit_resources_changed(&ctxt).await;
    }

    /// Dbus interface to remove all entries from config manager
    pub async fn remove_all(&mut self, #[zbus(signal_context)] ctxt: SignalContext<'_>) {
        let num_of_resources = self.resources.len();
        self.resources.clear();
        if num_of_resources > 0 {
            self.emit_resources_changed(&ctxt).await;
        }
        self.logger.warn("All resources cleared");
    }

    /// Dbus interface to remove single entry from config manager
    pub async fn remove_one(&mut self, #[zbus(signal_context)] ctxt: SignalContext<'_>, key: &str) {
        let key_normalized = normalize_key(key);
        let removed_entry = xrm::find_spec(&self.resources, &key_normalized)
            .cloned()
            .and_then(|k| self.handle_remove_one(&k));
        if let Some(pair) = removed_entry {
            self.emit_resources_changed(&ctxt).await;
            self.logger.warn(&format!("Resource cleared {:?}", pair));
        }
    }

    /// DBus interface for getting resources values
    #[zbus(property)]
    pub fn resources(&self) -> HashMap<String, String> {
        self.resources.clone()
    }
}

/// Resource name given over D-Bus, as stored: trimmed, and normalized if it
/// is a valid resource specification
fn normalize_key(key: &str) -> String {
    let key_trimmed = trim_str(key);
    xrm::normalize_spec(key_trimmed).unwrap_or_else(|| key_trimmed.to_string())
}

/// Inserts a resource, replacing the entry with the same specification
fn merge_resource(resources: &mut HashMap<String, String>, key: String, value: String) {
    if let Some(old_key) = xrm::find_spec(resources, &key).cloned() {
        resources.remove(&old_key);
    }
    resources.insert(key, value);
}

mod common;
pub mod input;
pub mod log;
pub mod parser;
pub mod tests;

use common::trim_str;
use log::Logger;
use parser::CliArgs;
use std::{
    collections::HashMap,
    error::Error,
    sync::{Arc, Mutex, MutexGuard},
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
        let service = Trawl {
            preprocessor: self.preprocessor.clone(),
            manager: Arc::new(Mutex::new(self)),
        };
        connection::Builder::session()?
            .name("org.regolith.Trawl")?
            .serve_at("/org/regolith/Trawl", service)?
            .build()
            .await
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
        Ok(input::read_file(file_path, nocpp, cpp, cpp_args)?)
    }

    /// Checks if a given key is a valid resource name
    fn check_valid_key(&self, key: &str) -> bool {
        let mut is_valid = true;
        let allowed_chars = ['-', '.', '_'];
        if key.len() == 0 {
            is_valid = false;
        }
        for ch in key.chars() {
            if !ch.is_ascii_alphanumeric() && !allowed_chars.contains(&ch) {
                self.logger.warn(&format!("{key} is not a valid key"));
                is_valid = false;
                break;
            }
        }
        is_valid
    }

    /// Parse the 'config_str' string into key value pairs
    fn parse_config(&self, config_str: &str) -> HashMap<String, String> {
        let parsed_resources: HashMap<String, String> = config_str
            .lines()
            // Split at ':'
            .filter_map(|s| s.split_once(':'))
            // trim and conver to String
            .map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))
            // Skip line if key contains invalid characters
            .filter(|(k, _)| self.check_valid_key(&k))
            .collect();
        parsed_resources
    }

    /// Adds the resources in `config_str` that aren't defined yet
    fn apply_load(&mut self, config_str: &str) {
        let parsed_resources = self.parse_config(config_str);
        self.logger
            .info(&format!("parsed_resources: {:#?}", parsed_resources));
        for (k, v) in parsed_resources {
            self.resources.entry(k).or_insert(v);
        }
        self.logger.info(&format!(
            "Updated resources after loading: \n {:#?}",
            &self.resources
        ))
    }

    /// Adds the resources in `config_str`, replacing existing values
    fn apply_merge(&mut self, config_str: &str) {
        let parsed_resources = self.parse_config(config_str);
        self.logger
            .info(&format!("parsed_resources: {:#?}", parsed_resources));
        for (k, v) in parsed_resources {
            self.resources.insert(k, v);
        }
        self.logger.info(&format!(
            "Updated resources after merging: \n {:#?}",
            &self.resources
        ))
    }

    /// Loads resources from the file. Doesn't override existing resources.
    fn load_from_file(&mut self, file: &str, nocpp: bool, cpp: &str, args: &[String]) {
        match self.get_preprocessed_file(file, nocpp, cpp, args) {
            Ok(conf) => self.apply_load(&conf),
            Err(e) => self.logger.from_error(e),
        }
    }

    /// Merges resources from the file with the loaded resources. Overrides
    /// value if key already presen in resources.
    #[cfg_attr(not(test), allow(dead_code))]
    fn merge_from_file(&mut self, file: &str, nocpp: bool, cpp: &str, args: &[String]) {
        match self.get_preprocessed_file(file, nocpp, cpp, args) {
            Ok(conf) => self.apply_merge(&conf),
            Err(e) => self.logger.from_error(e),
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

    /// Returns all the matching
    pub fn query(&self, q: &str) -> String {
        let query_trimmed = trim_str(q);
        let mut matches: Vec<_> = self
            .resources
            .iter()
            .filter(|(k, _)| k.contains(query_trimmed))
            .map(|(x, v)| format!("{} :\t{}", x, v))
            .collect();
        matches.sort();
        let query_result = matches.join("\n");
        self.logger.info(&format!(
            "Following resources match the query '{query_trimmed}'\
                                  : {query_result}"
        ));
        query_result
    }

    /// Get the resource value
    pub fn get_resource(&self, key: &str) -> String {
        let key_trimmed = trim_str(key);
        let value = self
            .resources
            .get(key_trimmed)
            .unwrap_or(&String::from(""))
            .to_owned();
        self.logger
            .info(&format!("value of key {key_trimmed} is {value}"));
        value
    }

    /// Sets a resource; returns true if anything changed
    fn handle_set(&mut self, key: &str, val: &str) -> bool {
        let key_trimmed = common::trim_str(key);
        let val_trimmed = common::trim_str(val);
        // Do not add key-value pair if key exists and current value is
        // same as the value to be inserted
        if let Some(x) = self.resources.get(key_trimmed) {
            if *x == val {
                return false;
            }
        }
        self.resources
            .insert(String::from(key_trimmed), String::from(val_trimmed));
        true
    }

    /// Adds a resource unless defined; returns true if anything changed
    fn handle_add(&mut self, key: &str, val: &str) -> bool {
        let key_trimmed = trim_str(key);
        let val_trimmed = trim_str(val);
        if self.resources.contains_key(key_trimmed) {
            return false;
        }
        self.resources
            .insert(String::from(key_trimmed), String::from(val_trimmed));
        true
    }
}

/// Splits the argument string of the `LoadCpp` and `MergeCpp` methods.
fn split_legacy_args(args: &str) -> Vec<String> {
    args.split_whitespace().map(String::from).collect()
}

/// Load or merge
#[derive(Clone, Copy)]
enum Mode {
    Load,
    Merge,
}

/// The D-Bus object. Every method takes `&self`: the object lock is shared,
/// and the resources are behind a mutex that is only held for in-memory work.
/// Reading input (which can block for any time) happens on a blocking task
/// without the mutex, so a stuck input never delays other callers.
pub struct Trawl {
    manager: Arc<Mutex<ResourceManager>>,
    preprocessor: String,
}

impl Trawl {
    fn manager(&self) -> MutexGuard<'_, ResourceManager> {
        self.manager.lock().unwrap_or_else(|p| p.into_inner())
    }

    async fn emit_resources_changed(&self, ctxt: &SignalContext<'_>) {
        if let Err(e) = self.resources_changed(ctxt).await {
            self.manager().logger.error(&format!("{e}"));
        }
    }

    fn apply(&self, mode: Mode, text: &str) {
        let mut manager = self.manager();
        match mode {
            Mode::Load => manager.apply_load(text),
            Mode::Merge => manager.apply_merge(text),
        }
    }

    async fn from_file(
        &self,
        ctxt: &SignalContext<'_>,
        mode: Mode,
        path: &str,
        nocpp: bool,
        cpp: &str,
        args: Vec<String>,
    ) {
        let (path, cpp) = (path.to_owned(), cpp.to_owned());
        let task_path = path.clone();
        let read =
            tokio::task::spawn_blocking(move || input::read_file(&task_path, nocpp, &cpp, &args))
                .await
                .unwrap_or_else(|e| Err(format!("reading '{path}' failed: {e}")));
        match read {
            Ok(text) => {
                self.apply(mode, &text);
                self.emit_resources_changed(ctxt).await;
            }
            Err(e) => self.manager().logger.error(&e),
        }
    }
}

#[interface(name = "org.regolith.trawl1")]
impl Trawl {
    /// DBus Interface to load resources from file
    async fn load(&self, #[zbus(signal_context)] ctxt: SignalContext<'_>, path: &str, nocpp: bool) {
        let cpp = self.preprocessor.clone();
        self.from_file(&ctxt, Mode::Load, path, nocpp, &cpp, Vec::new())
            .await
    }

    /// DBus Interface to merge resources from file
    async fn merge(
        &self,
        #[zbus(signal_context)] ctxt: SignalContext<'_>,
        path: &str,
        nocpp: bool,
    ) {
        let cpp = self.preprocessor.clone();
        self.from_file(&ctxt, Mode::Merge, path, nocpp, &cpp, Vec::new())
            .await
    }

    /// DBus Interface to load resources from file using custom preprocessor.
    /// `args` is split on whitespace, so an argument can't contain a space;
    /// use `LoadCppArgv` instead.
    async fn load_cpp(
        &self,
        #[zbus(signal_context)] ctxt: SignalContext<'_>,
        path: &str,
        cpp: &str,
        args: &str,
    ) {
        self.from_file(&ctxt, Mode::Load, path, false, cpp, split_legacy_args(args))
            .await
    }

    /// DBus Interface to merge resources from file using custom preprocessor.
    /// `args` is split on whitespace, so an argument can't contain a space;
    /// use `MergeCppArgv` instead.
    async fn merge_cpp(
        &self,
        #[zbus(signal_context)] ctxt: SignalContext<'_>,
        path: &str,
        cpp: &str,
        args: &str,
    ) {
        self.from_file(
            &ctxt,
            Mode::Merge,
            path,
            false,
            cpp,
            split_legacy_args(args),
        )
        .await
    }

    /// DBus Interface to load resources from file using custom preprocessor.
    /// Each element of `args` is passed to the preprocessor as one argument.
    async fn load_cpp_argv(
        &self,
        #[zbus(signal_context)] ctxt: SignalContext<'_>,
        path: &str,
        cpp: &str,
        args: Vec<String>,
    ) {
        self.from_file(&ctxt, Mode::Load, path, false, cpp, args)
            .await
    }

    /// DBus Interface to merge resources from file using custom preprocessor.
    /// Each element of `args` is passed to the preprocessor as one argument.
    async fn merge_cpp_argv(
        &self,
        #[zbus(signal_context)] ctxt: SignalContext<'_>,
        path: &str,
        cpp: &str,
        args: Vec<String>,
    ) {
        self.from_file(&ctxt, Mode::Merge, path, false, cpp, args)
            .await
    }

    /// Returns all the matching
    fn query(&self, q: &str) -> String {
        self.manager().query(q)
    }

    /// Get the resource value
    fn get_resource(&self, key: &str) -> String {
        self.manager().get_resource(key)
    }

    /// DBus interface to set the value of a resource. Overwrites exiting value.
    /// # Emits
    /// **resources_changed**: This signal indicated the change
    /// in'resources' property value
    async fn set_resource(
        &self,
        #[zbus(signal_context)] ctxt: SignalContext<'_>,
        key: String,
        val: String,
    ) {
        let changed = self.manager().handle_set(&key, &val);
        if changed {
            self.emit_resources_changed(&ctxt).await;
        }
    }

    /// DBus interface to add a new resource. Doesn't overwrite exiting
    /// values in case of conflicts
    async fn add_resource(
        &self,
        #[zbus(signal_context)] ctxt: SignalContext<'_>,
        key: String,
        val: String,
    ) {
        let changed = self.manager().handle_add(&key, &val);
        if changed {
            self.emit_resources_changed(&ctxt).await;
        }
    }

    /// Dbus interface to remove all entries from config manager
    async fn remove_all(&self, #[zbus(signal_context)] ctxt: SignalContext<'_>) {
        let num_of_resources = {
            let mut manager = self.manager();
            let n = manager.resources.len();
            manager.handle_remove_all();
            manager.logger.warn("All resources cleared");
            n
        };
        if num_of_resources > 0 {
            self.emit_resources_changed(&ctxt).await;
        }
    }

    /// Dbus interface to remove single entry from config manager
    async fn remove_one(&self, #[zbus(signal_context)] ctxt: SignalContext<'_>, key: &str) {
        let removed_entry = {
            let mut manager = self.manager();
            let removed = manager.handle_remove_one(trim_str(key));
            if let Some(pair) = &removed {
                manager.logger.warn(&format!("Resource cleared {:?}", pair));
            }
            removed
        };
        if removed_entry.is_some() {
            self.emit_resources_changed(&ctxt).await;
        }
    }

    /// DBus interface for getting resources values
    #[zbus(property)]
    fn resources(&self) -> HashMap<String, String> {
        self.manager().resources.clone()
    }
}

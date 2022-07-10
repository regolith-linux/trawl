pub mod parser;
pub mod log;
pub mod tests;

use std::{process::Command, collections::HashMap, error::Error, fs};
use parser::CliArgs;
use log::Logger;
use zbus::{dbus_interface, SignalContext, ConnectionBuilder};


/// Stores and manages the resources
#[derive( Debug, PartialEq, Eq)]
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
                None => return
            }
        };
        self.load_from_file(&filename.clone(), self.args.nocpp, &self.preprocessor.clone());
    }

    /// Runs the config daemon
    pub async fn run_server(self) -> zbus::Result<()> {
        ConnectionBuilder::session()?
            .name("org.regolith.ConfigMgr")?
            .serve_at("/org/regolith/ConfigMgr", self)?
            .build()
            .await?;
        Ok(())

    }

    /// Getter for preprocessor
    fn preprocessor(&self, cmd: &str) -> Command{
        Command::new(cmd)
    }

    /// Returns the content of the file after preprocessing
    fn get_preprocessed_file(&mut self, file_path: &str, nocpp: bool, cpp: &str) -> Result<String, Box<dyn Error>> {
        if nocpp {
            self.logger.warn("wont use preprocessor - try running without --nocpp flag");
            let config_str = fs::read_to_string(file_path)?;
            self.logger.info("Config file read successfully");
            self.logger.info(&config_str);
            return Ok(config_str);
        }
        let output_bytes = self.preprocessor(cpp)
            .arg(file_path)
            .output()?
            .stdout;


        let conf_utf8 = String::from_utf8(output_bytes)?;
        self.logger.info("File preprocessed successfully...");
        self.logger.info(&conf_utf8);
        Ok(conf_utf8)
    }

    /// Checks if a given key is a valid resource name
    fn check_valid_key(&self, key: &str) -> bool {
        let allowed_chars = ['-', '.', '_'];
        for ch in key.chars() {
            if !ch.is_ascii_alphanumeric() && !allowed_chars.contains(&ch) {
                self.logger.warn(&format!("{key} is not a valid key"));
                return false;
            }
        }
        true
    }

    /// Parse the 'config_str' string into key value pairs
    fn parse_config(&self, config_str: &str) -> HashMap<String, String> {
        let parsed_resources: HashMap<String, String>= config_str.lines()
            // Split at ':'
            .filter_map(|s| s.split_once(':')) 
            // trim and conver to String
            .map(|(k, v)| (k.trim().to_string(), v.trim().to_string())) 
            // Skip line if key contains invalid characters
            .filter(|(k, _)| self.check_valid_key(&k))
            .collect();
        parsed_resources
    }

    /// Loads resources from the file. Doesn't override existing resources.
    fn load_from_file(&mut self, file: &str, nocpp: bool, cpp: &str) {
        let config_str = match self.get_preprocessed_file(file, nocpp, cpp) {
            Ok(conf) => conf,
            Err(e) => {
                self.logger.from_error(e);
                return;
            }
        }; 
        let parsed_resources = self.parse_config(&config_str);
        self.logger.info(&format!("parsed_resources: {:#?}", parsed_resources));
        for (k, v) in parsed_resources {
            self.resources.entry(k).or_insert(v);
        }
        self.logger.info(&format!("Updated resources after loading: \n {:#?}", &self.resources))
    }


    /// Merges resources from the file with the loaded resources. Overrides 
    /// value if key already presen in resources.
    fn merge_from_file(&mut self, file: &str, nocpp: bool, cpp: &str)  {
        let config_str = match self.get_preprocessed_file(file, nocpp, cpp) {
            Ok(conf) => conf,
            Err(e) => {
                self.logger.from_error(e);
                return;
            }
        }; 
        let parsed_resources = self.parse_config(&config_str);
        self.logger.info(&format!("parsed_resources: {:#?}", parsed_resources));
        for (k, v) in parsed_resources {
            self.resources.insert(k, v);
        }
        self.logger.info(&format!("Updated resources after merging: \n {:#?}", &self.resources))
    }


    /// Notify clients when resource changed
    pub async fn emit_resources_changed (&self, ctxt: &SignalContext<'_>) {
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

#[dbus_interface(name = "org.regolith.config1")]
impl ResourceManager {
    /// DBus Interface to load resources from file
    async fn load(
        &mut self,
        #[zbus(signal_context)]
        ctxt: SignalContext<'_>,
        path: &str,
        nocpp: bool
    ){
        self.load_from_file(path, nocpp, &self.preprocessor.clone());
        self.emit_resources_changed(&ctxt).await;
    }

    /// DBus Interface to merge resources from file
    async fn merge(
        &mut self,
        #[zbus(signal_context)]
        ctxt: SignalContext<'_>,
        path: &str,
        nocpp: bool
    ){
        self.merge_from_file(path, nocpp, &self.preprocessor.clone());
        self.emit_resources_changed(&ctxt).await;
    }

    /// DBus Interface to load resources from file using custom preprocessor
    async fn load_cpp(
        &mut self,
        #[zbus(signal_context)]
        ctxt: SignalContext<'_>,
        path: &str,
        cpp: &str
    ){
        self.load_from_file(path, false, cpp);
        self.emit_resources_changed(&ctxt).await;
    }

    /// DBus Interface to merge resources from file using custom preprocessor
    async fn merge_cpp(
        &mut self,
        #[zbus(signal_context)]
        ctxt: SignalContext<'_>,
        path: &str,
        cpp: &str
    ){
        self.merge_from_file(path, false, cpp);
        self.emit_resources_changed(&ctxt).await;
    }

    /// Returns all the matching 
    /// *Note*: Also a DBus interface
    pub fn query(&self, q: &str) -> String {
        let mut matches:Vec<_> = self.resources.iter()
            .filter(|(k, _)| k.contains(q))
            .map(|(x, v)| format!("{} :\t{}", x, v))
            .collect();
        matches.sort();
        let query_result = matches.join("\n");
        self.logger.info(&format!("Following queries match the query {q}
                                  :\n{query_result}"));

        query_result
    }

    /// Get the resource value
    pub fn get_resource(&self, key: &str) -> String {
        self.resources
            .get(key)
            .unwrap_or(&String::from(""))
            .to_owned()
    }

    /// DBus interface to set the value of a resource. Overwrites exiting value.
    /// TODO: Separate implementation to make more testable
    /// # Emits
    /// **resources_changed**: This signal indicated the change 
    /// in'resources' property value 
    pub async fn set_resource(
        &mut self, 
        #[zbus(signal_context)]
        ctxt: SignalContext<'_>,
        key: String, 
        val: String
    ) {
        let curr_val = self.resources.get(&key);
        // Do not add key-value pair if key exists and current value is
        // same as the value to be inserted
        if let Some(x) = curr_val {
            if *x == val {
                return
            }
        }
        self.resources.insert(key, val);
        self.emit_resources_changed(&ctxt).await;

    }

    /// DBus interface to add a new resource. Doesn't overwrite exiting 
    /// values in case of conflicts
    /// TODO: Separate implementation to make more testable
    pub async fn add_resource(
        &mut self, 
        #[zbus(signal_context)]
        ctxt: SignalContext<'_>,
        key: String, 
        val: String,
    ) {
        let curr_val = self.resources.get(&key);
        // Do not add key value pair if key already defined
        if let Some(_) = curr_val {
            return;
        }
        self.resources.insert(key, val);
        self.emit_resources_changed(&ctxt).await;
    }

    /// Dbus interface to remove all entries from config manager
    pub async fn remove_all(
        &mut self, 
        #[zbus(signal_context)]
        ctxt: SignalContext<'_>,
    ) {
        let num_of_resources = self.resources.len();
        self.resources.clear();
        if num_of_resources > 0 {
            self.emit_resources_changed(&ctxt).await;
        }
        self.logger.warn("All resources cleared");
    }

    /// Dbus interface to remove single entry from config manager
    pub async fn remove_one(
        &mut self, 
        #[zbus(signal_context)]
        ctxt: SignalContext<'_>,
        key: &str
    ) {
        let removed_entry = self.handle_remove_one(key);
        if let Some(pair) = removed_entry {
            self.emit_resources_changed(&ctxt).await;
            self.logger.warn(&format!("Resource cleared {:?}", pair));
        }
    }

    /// DBus interface for getting resources values
    #[dbus_interface(property)]
    pub fn resources(&self) -> HashMap<String, String> {
        self.resources.clone()
    }
}

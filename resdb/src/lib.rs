pub mod manager;
pub mod parser;

use std::{fs::{self, File}, error::Error, path::Path, io::Write};
use parser::CliArgs;
use zbus::{Connection, fdo::PropertiesProxy, export::futures_util::StreamExt, zvariant::Value};
use manager::ResourceManagerProxy;

pub struct Client <'a> {
    connection: Connection,
    proxy: ResourceManagerProxy<'a>,
}

impl<'a> Client<'a> {
    pub async fn new() -> Result<Client<'a>, Box<dyn Error>> {
        let connection = Connection::session().await?;
        let proxy = ResourceManagerProxy::new(&connection).await?;
        let client = Client {
            connection,
            proxy,
        };
        Ok(client)
    }
    pub fn proxy(&self) -> &ResourceManagerProxy {
        &self.proxy
    }
    /// Listen for changes in properties. Uses callback to perform required actions.
    /// # Example
    /// ```
    /// use zbus::zvariant::Value;
    /// async {
    ///     let client = resdb::Client::new().await.unwrap();
    ///     let mut callback = |s:String, v: Value| println!("{s} {:#?}", v);
    ///     client.listen_props_change(callback);
    /// };
    /// ```
    pub async fn listen_props_change<T>(&self, mut callback: T) -> Result<(), Box<dyn Error>> 
    where
        T: FnMut(String, Value) -> ()
    {
        let props = PropertiesProxy::builder(&self.connection)
            .destination("org.regolith.ConfigMgr")?
            .path(self.proxy.path())?
            .build()
            .await?;
        let mut props_changed = props.receive_properties_changed().await?; 
        while let Some(signal) = props_changed.next().await {
            let args = signal.args()?;
            for (&name, value) in args.changed_properties().iter() {
                callback(name.to_owned(), value.clone());
            }
        }
        Ok(())
    }

    fn get_absolute_path(relative_path: &str) -> Result<String, Box<dyn Error>> {
        let path_buf = fs::canonicalize(relative_path)?;
        let abs = path_buf.to_str()
            .expect("Failed to get absolute path: UTF-8 conversion failed")
            .to_owned();
        Ok(abs)
    }

    async fn load(&self, path: &str, nocpp: bool, cpp: &Option<String>) -> Result<(), Box<dyn Error>> {
        let abs_path = Self::get_absolute_path(path)?;
        if let Some(preprocessor) = &cpp {
            self.proxy.load_cpp(&abs_path, preprocessor).await?;
        }
        else {
            self.proxy.load(&abs_path, nocpp).await?;
        }
        Ok(())
    }

    async fn merge(&self, path: &str, nocpp: bool, cpp: &Option<String>) -> Result<(), Box<dyn Error>> {
        let abs_path = Self::get_absolute_path(path)?;
        if let Some(preprocessor) = cpp {
            self.proxy.merge_cpp(&abs_path, preprocessor).await?;
        } else {
            self.proxy.merge(&abs_path, nocpp).await?;
        }
        Ok(())
    }

    async fn query(&self, q: &str) -> zbus::Result<()> {
        let result = self.proxy.query(q).await?;
        print!("{result}");
        std::io::stdout().flush()?;
        Ok(())
    }

    async fn get_resource(&self, key: &str) -> zbus::Result<()> {
        let result = self.proxy.get_resource(key).await?;
        println!("{result}");
        Ok(())
    }

    pub async fn run(&self, args: &CliArgs)  -> Result<(), Box<dyn Error>> {
        // Load file (default)
        if args.load.is_some() || args.filename.is_some() {
            let file = match &args.load {
                Some(file) => file,
                None => args.filename.as_ref().unwrap(),
            };
            self.load(file, args.nocpp, &args.cpp).await?;
        }
        // Merge resources
        if let Some(file) = &args.merge {
            self.merge(&file, args.nocpp, &args.cpp).await?;
        }
        // Either query all resources or get single resource
        if let Some(query) = &args.query {
            let query_string = if query.len() == 0 {
                ""
            } else {
                &query[0]
            };
            self.query(query_string).await?;
        }
        else if let Some(key) = &args.get {
            self.get_resource(&key).await?;
        }
        // Edit into file 
        if let Some(path) = &args.edit {
            let path_obj = Path::new(path);
            // Backup exiting file with same name, if exists
            if path_obj.exists() {
                let ext = match &args.backup {
                    Some(e) => e,
                    None => ".bak"
                };
                let mut bak_name = path_obj.as_os_str().to_owned();
                bak_name.push(ext);
                fs::rename(path_obj, bak_name)?;
            }
            let mut file_handle = File::create(path_obj)?;
            let res_str = self.proxy.query("").await?;
            file_handle.write_all(res_str.as_bytes())?;
        } 
        if args.remove {
            self.proxy().remove_all().await?;
        }
        Ok(())
    }
}

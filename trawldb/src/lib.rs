pub mod manager;
pub mod parser;

use manager::ResourceManagerProxy;
use parser::CliArgs;
use std::{
    error::Error,
    fs::{self, File},
    io::Write,
    path::Path,
};
use zbus::{
    export::futures_util::StreamExt,
    fdo::{PropertiesChanged, PropertiesProxy},
    zvariant::Value,
    Connection,
};

pub struct Client<'a> {
    connection: Connection,
    proxy: ResourceManagerProxy<'a>,
}

impl<'a> Client<'a> {
    pub async fn new() -> Result<Client<'a>, Box<dyn Error>> {
        let connection = Connection::session().await?;
        let proxy = ResourceManagerProxy::new(&connection).await?;
        let client = Client { connection, proxy };
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
    ///     let client = trawldb::Client::new().await.unwrap();
    ///     let mut callback = |s:String, v: Value| println!("{s} {:#?}", v);
    ///     client.listen_props_change(callback);
    /// };
    /// ```
    pub async fn listen_props_change<T>(&self, mut callback: T) -> Result<(), Box<dyn Error>>
    where
        T: FnMut(String, Value) -> (),
    {
        loop {
            let signal = self.get_props_changed_signal().await?;
            let args = signal.args()?;
            for (&name, value) in args.changed_properties().iter() {
                callback(name.to_owned(), value.clone());
            }
        }
    }

    pub async fn get_props_changed_signal(&self) -> Result<PropertiesChanged, Box<dyn Error>> {
        let props = PropertiesProxy::builder(&self.connection)
            .destination("org.regolith.Trawl")?
            .path(self.proxy.path())?
            .build()
            .await?;
        let mut props_changed = props.receive_properties_changed().await?;
        loop {
            if let Some(signal) = props_changed.next().await {
                return Ok(signal);
            }
        }
    }

    fn get_absolute_path(relative_path: &str) -> Result<String, Box<dyn Error>> {
        let path_buf = fs::canonicalize(relative_path)?;
        let abs = path_buf
            .to_str()
            .expect("Failed to get absolute path: UTF-8 conversion failed")
            .to_owned();
        Ok(abs)
    }

    fn get_cpp_with_args(
        cpp: &Option<String>,
        include: &Vec<String>,
        define: &Vec<String>,
    ) -> Option<(String, String)> {
        let include_args = include
            .iter()
            .filter_map(|s| Self::get_absolute_path(s).ok())
            .map(|s| format!("-I {s}"));
        let define_args = define.iter().map(|s| format!("-D {s}"));
        let args_list: Vec<String> = include_args.chain(define_args).collect();
        let args_str = args_list.join(" ");
        let new_cpp = match cpp {
            Some(preprocessor) if args_list.len() > 0 => Some((preprocessor.clone(), args_str)),
            None if args_list.len() > 0 => Some((String::from("/usr/bin/cpp"), args_str)),
            Some(preprocessor) => Some((preprocessor.clone(), String::from(""))),
            _ => None,
        };
        new_cpp
    }

    async fn load(
        &self,
        path: &str,
        nocpp: bool,
        cpp: &Option<String>,
        include: &Vec<String>,
        define: &Vec<String>,
    ) -> Result<(), Box<dyn Error>> {
        let abs_path = Self::get_absolute_path(path)?;
        let cpp_with_args = Self::get_cpp_with_args(cpp, include, define);
        if let Some((preprocessor, args)) = &cpp_with_args {
            println!("cpp: {preprocessor} args: {args}");
            self.proxy.load_cpp(&abs_path, preprocessor, args).await?;
        } else {
            self.proxy.load(&abs_path, nocpp).await?;
        }
        Ok(())
    }

    async fn merge(
        &self,
        path: &str,
        nocpp: bool,
        cpp: &Option<String>,
        include: &Vec<String>,
        define: &Vec<String>,
    ) -> Result<(), Box<dyn Error>> {
        let abs_path = Self::get_absolute_path(path)?;
        let cpp_with_args = Self::get_cpp_with_args(cpp, include, define);
        if let Some((preprocessor, args)) = &cpp_with_args {
            self.proxy.merge_cpp(&abs_path, preprocessor, args).await?;
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

    pub async fn run(&self, args: &CliArgs) -> Result<(), Box<dyn Error>> {
        // Load file (default)
        if args.load.is_some() || args.filename.is_some() {
            let file = match &args.load {
                Some(file) => file,
                None => args.filename.as_ref().unwrap(),
            };
            self.load(file, args.nocpp, &args.cpp, &args.include, &args.define)
                .await?;
        }
        // Merge resources
        if let Some(file) = &args.merge {
            self.merge(&file, args.nocpp, &args.cpp, &args.include, &args.define)
                .await?;
        }
        // Either query all resources or get single resource
        if let Some(query) = &args.query {
            let query_string = if query.len() == 0 { "" } else { &query[0] };
            self.query(query_string).await?;
        } else if let Some(key) = &args.get {
            self.get_resource(&key).await?;
        }
        // Edit into file
        if let Some(path) = &args.edit {
            let path_obj = Path::new(path);
            // Backup exiting file with same name, if exists
            if path_obj.exists() {
                let ext = match &args.backup {
                    Some(e) => e,
                    None => ".bak",
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

#[cfg(test)]
mod tests {
    use crate::Client;
    #[test]
    fn it_checks_get_cpp_args() {
        let cpp = None;
        let args = Client::get_cpp_with_args(&cpp, &Vec::new(), &Vec::new());
        assert_eq!(args, None);

        let cpp = String::from("/usr/bin/cpp");
        let args = Client::get_cpp_with_args(&Some(cpp.clone()), &Vec::new(), &Vec::new());
        assert_eq!(args, Some((cpp.clone(), String::new())));

        let include = vec![String::from(".")];
        let define = vec![String::from("MODKEY=Mod4"), String::from("SWAY")];
        println!("include: {:?}", include);
        println!("define: {:?}", define);

        let args = Client::get_cpp_with_args(&Some(cpp.clone()), &include, &define).unwrap();
        assert_eq!(args.clone().0, cpp.clone());
        assert_eq!(
            args.1,
            format!(
                "-I {} -D MODKEY=Mod4 -D SWAY",
                Client::get_absolute_path(".").unwrap()
            )
        )
    }
}

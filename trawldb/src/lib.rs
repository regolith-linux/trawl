pub mod cpp;
pub mod manager;
pub mod parser;

use manager::ResourceManagerProxy;
use parser::CliArgs;
use std::{
    error::Error,
    fs::{self, File},
    io::{Read, Write},
    path::{self as stdpath, Path},
    process::Stdio,
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
                callback(name.to_owned(), value.try_clone()?);
            }
        }
    }

    pub async fn get_props_changed_signal(&self) -> Result<PropertiesChanged, Box<dyn Error>> {
        let props = PropertiesProxy::builder(&self.connection)
            .destination("org.regolith.Trawl")?
            .path("/org/regolith/Trawl")?
            .build()
            .await?;
        let mut props_changed = props.receive_properties_changed().await?;
        loop {
            if let Some(signal) = props_changed.next().await {
                return Ok(signal);
            }
        }
    }

    /// Makes a path absolute without resolving symlinks
    fn get_absolute_path(relative_path: &str) -> Result<String, Box<dyn Error>> {
        let path_buf = stdpath::absolute(relative_path)?;
        let abs = path_buf
            .to_str()
            .ok_or("Failed to get absolute path: UTF-8 conversion failed")?
            .to_owned();
        Ok(abs)
    }

    /// The preprocessor to run and its arguments, or None with --nocpp
    fn get_cpp_with_args(
        nocpp: bool,
        cpp: &Option<String>,
        include: &Vec<String>,
        define: &Vec<String>,
    ) -> Option<(String, Vec<String>)> {
        if nocpp {
            return None;
        }
        let include_args = include
            .iter()
            .filter_map(|s| Self::get_absolute_path(s).ok())
            .flat_map(|s| [String::from("-I"), s]);
        let define_args = define.iter().flat_map(|s| [String::from("-D"), s.clone()]);
        let args_list: Vec<String> = include_args.chain(define_args).collect();
        let preprocessor = cpp.clone().unwrap_or_else(|| String::from("/usr/bin/cpp"));
        Some((preprocessor, args_list))
    }

    /// Reads resource input like xrdb: `-` (or no file) is stdin, anything
    /// else is a path used as given. The preprocessor runs here, in the
    /// client, so its diagnostics go to our stderr and a blocked input only
    /// blocks this process. Bytes that aren't valid UTF-8 become U+FFFD.
    /// None when the preprocessor can't be run: like xrdb, that is reported
    /// on stderr and nothing is taken in, but it isn't a failure.
    fn read_input(
        file: Option<&str>,
        nocpp: bool,
        cpp: &Option<String>,
        include: &Vec<String>,
        define: &Vec<String>,
    ) -> Result<Option<String>, Box<dyn Error>> {
        let cpp_with_args = Self::get_cpp_with_args(nocpp, cpp, include, define);
        let path = file.filter(|f| *f != "-");
        let name = path.unwrap_or("-");
        // Read once, whole: a FIFO can't be opened twice.
        let mut buf = Vec::new();
        let read = match path {
            Some(p) => {
                let mut f = File::open(p).map_err(|e| format!("can't open file '{p}': {e}"))?;
                if f.metadata().map(|m| m.is_dir()).unwrap_or(false) {
                    return Err(format!("can't read file '{p}': it is a directory").into());
                }
                f.read_to_end(&mut buf)
            }
            None => std::io::stdin().lock().read_to_end(&mut buf),
        };
        read.map_err(|e| format!("can't read file '{name}': {e}"))?;
        let Some((preprocessor, args)) = cpp_with_args else {
            return Ok(Some(String::from_utf8_lossy(&buf).into_owned()));
        };
        // Quoted includes resolve against the directory of the path as given
        // (symlinks are not resolved first); stdin's resolve against the
        // current directory, which cpp searches anyway. Diagnostics go
        // straight to our stderr.
        let dir = path
            .map(Path::new)
            .and_then(Path::parent)
            .filter(|dir| !dir.as_os_str().is_empty());
        match cpp::preprocess(&preprocessor, &args, dir, &buf, Stdio::inherit()) {
            Ok((text, _)) => Ok(Some(text)),
            Err(cpp::CppError::NotRun(e)) => {
                eprintln!("trawldb: {e}");
                Ok(None)
            }
            Err(cpp::CppError::Failed(e)) => Err(e.into()),
        }
    }

    /// Reads the input off the async runtime: it may block for any time.
    async fn read_input_blocking(
        file: Option<&str>,
        nocpp: bool,
        cpp: &Option<String>,
        include: &Vec<String>,
        define: &Vec<String>,
    ) -> Result<Option<String>, Box<dyn Error>> {
        let (file, cpp) = (file.map(String::from), cpp.clone());
        let (include, define) = (include.clone(), define.clone());
        tokio::task::spawn_blocking(move || {
            Client::read_input(file.as_deref(), nocpp, &cpp, &include, &define)
                .map_err(|e| e.to_string())
        })
        .await?
        .map_err(Into::into)
    }

    /// True if trawld doesn't know the method, i.e. it predates it
    fn is_unknown_method(e: &zbus::Error) -> bool {
        matches!(e, zbus::Error::MethodError(name, _, _)
            if name.as_str() == "org.freedesktop.DBus.Error.UnknownMethod")
    }

    /// Joins the arguments for the older LoadCpp/MergeCpp methods, which
    /// split them on whitespace. Fails if that would change the arguments.
    fn get_legacy_args(args: &[String]) -> Result<String, Box<dyn Error>> {
        if args
            .iter()
            .any(|a| a.is_empty() || a.chars().any(char::is_whitespace))
        {
            return Err("trawld is too old for empty preprocessor arguments \
                        or ones with spaces; update trawld"
                .into());
        }
        Ok(args.join(" "))
    }

    /// For a trawld that predates LoadText/MergeText (one still running
    /// from before an upgrade): has trawld read the file itself, as before.
    /// It can't read our stdin.
    async fn from_file_old_trawld(
        &self,
        merge: bool,
        path: Option<&str>,
        nocpp: bool,
        cpp: &Option<String>,
        include: &Vec<String>,
        define: &Vec<String>,
    ) -> Result<(), Box<dyn Error>> {
        let path = path
            .filter(|p| *p != "-")
            .ok_or("trawld is too old to read standard input; update trawld")?;
        let abs_path = Self::get_absolute_path(path)?;
        let Some((preprocessor, args)) = Self::get_cpp_with_args(nocpp, cpp, include, define)
        else {
            return Ok(match merge {
                false => self.proxy.load(&abs_path, true).await?,
                true => self.proxy.merge(&abs_path, true).await?,
            });
        };
        let res = match merge {
            false => {
                self.proxy
                    .load_cpp_argv(&abs_path, &preprocessor, &args)
                    .await
            }
            true => {
                self.proxy
                    .merge_cpp_argv(&abs_path, &preprocessor, &args)
                    .await
            }
        };
        match res {
            Err(e) if Self::is_unknown_method(&e) => {
                let args = Self::get_legacy_args(&args)?;
                match merge {
                    false => self.proxy.load_cpp(&abs_path, &preprocessor, &args).await?,
                    true => {
                        self.proxy
                            .merge_cpp(&abs_path, &preprocessor, &args)
                            .await?
                    }
                }
            }
            res => res?,
        }
        Ok(())
    }

    async fn load(
        &self,
        path: Option<&str>,
        nocpp: bool,
        cpp: &Option<String>,
        include: &Vec<String>,
        define: &Vec<String>,
    ) -> Result<(), Box<dyn Error>> {
        let Some(text) = Self::read_input_blocking(path, nocpp, cpp, include, define).await? else {
            return Ok(());
        };
        match self.proxy.load_text(&text).await {
            Err(e) if Self::is_unknown_method(&e) => {
                self.from_file_old_trawld(false, path, nocpp, cpp, include, define)
                    .await?
            }
            res => res?,
        }
        Ok(())
    }

    async fn merge(
        &self,
        path: Option<&str>,
        nocpp: bool,
        cpp: &Option<String>,
        include: &Vec<String>,
        define: &Vec<String>,
    ) -> Result<(), Box<dyn Error>> {
        let Some(text) = Self::read_input_blocking(path, nocpp, cpp, include, define).await? else {
            return Ok(());
        };
        match self.proxy.merge_text(&text).await {
            Err(e) if Self::is_unknown_method(&e) => {
                self.from_file_old_trawld(true, path, nocpp, cpp, include, define)
                    .await?
            }
            res => res?,
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
        // Remove first, so that `--remove --load/--merge file` replaces the
        // resources with the file's instead of removing what was just added
        if args.remove {
            self.proxy().remove_all().await?;
        }
        // Load file (default)
        if args.load.is_some() || args.filename.is_some() {
            let file = match &args.load {
                Some(file) => file.as_deref(),
                None => args.filename.as_deref(),
            };
            self.load(file, args.nocpp, &args.cpp, &args.include, &args.define)
                .await?;
        }
        // Merge resources
        if let Some(file) = &args.merge {
            self.merge(
                file.as_deref(),
                args.nocpp,
                &args.cpp,
                &args.include,
                &args.define,
            )
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
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::Client;
    #[test]
    fn it_checks_get_cpp_args() {
        let cpp = None;
        let args = Client::get_cpp_with_args(true, &cpp, &Vec::new(), &Vec::new());
        assert_eq!(args, None);

        let args = Client::get_cpp_with_args(false, &cpp, &Vec::new(), &Vec::new());
        assert_eq!(args, Some((String::from("/usr/bin/cpp"), Vec::new())));

        let cpp = String::from("/usr/bin/cpp");
        let include = vec![String::from(".")];
        let define = vec![String::from("MODKEY=Mod4"), String::from("SWAY")];
        let args = Client::get_cpp_with_args(false, &Some(cpp.clone()), &include, &define).unwrap();
        assert_eq!(args.0, cpp);
        assert_eq!(
            args.1,
            vec![
                String::from("-I"),
                Client::get_absolute_path(".").unwrap(),
                String::from("-D"),
                String::from("MODKEY=Mod4"),
                String::from("-D"),
                String::from("SWAY"),
            ]
        )
    }

    #[test]
    fn it_keeps_cpp_args_with_spaces() {
        let mut dir = std::env::temp_dir();
        dir.push(format!("trawldb {} dir", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let dir = dir.to_str().unwrap().to_string();
        let include = vec![dir.clone()];
        let define = vec![String::from("NAME=a b")];
        let args = Client::get_cpp_with_args(false, &None, &include, &define).unwrap();
        assert_eq!(args.0, "/usr/bin/cpp");
        assert_eq!(
            args.1,
            vec![
                String::from("-I"),
                Client::get_absolute_path(&dir).unwrap(),
                String::from("-D"),
                String::from("NAME=a b"),
            ]
        );
        std::fs::remove_dir(&dir).unwrap();
    }

    #[test]
    fn it_checks_legacy_cpp_args() {
        let args = vec![String::from("-I"), String::from("/usr/share")];
        assert_eq!(Client::get_legacy_args(&args).unwrap(), "-I /usr/share");
        assert_eq!(Client::get_legacy_args(&[]).unwrap(), "");
        let args = vec![String::from("-D"), String::from("NAME=a b")];
        assert!(Client::get_legacy_args(&args).is_err());
        let args = vec![String::from("-D"), String::new()];
        assert!(Client::get_legacy_args(&args).is_err());
    }
}

#[cfg(test)]
pub mod unit_test {
    use crate::*;
    use std::{
        fs::{self, File},
        io::{self, Write},
        path::PathBuf,
    };
    use uuid::Uuid;

    #[allow(non_snake_case)]
    fn example_file_parsed() -> (&'static str, HashMap<String, String>) {
        const CONTENTS: &str = "\
home_dir:/home
    key_one:val_one\t
invalid-line's#!\\
lockscreen-timeout:10     
\tscreen-resolution:    1920x1080  
valid: \"line\"
\"hello\' : invalid
 /home: invalid
 string\\: invalid
    :   
        ";
        let expected_map: HashMap<_, _> = [
            ("home_dir", "/home"),
            ("key_one", "val_one"),
            ("lockscreen-timeout", "10"),
            ("valid", "\"line\""),
            ("screen-resolution", "1920x1080"),
        ]
        .iter()
        .map(|&(k, v)| (k.to_string(), v.to_string()))
        .collect();
        (CONTENTS, expected_map)
    }

    fn seed_args() -> CliArgs {
        CliArgs {
            load: None,
            cpp: None,
            nocpp: false,
            filename: None,
            debug: true,
            verbose: false,
        }
    }
    pub fn seed_resource_manager() -> ResourceManager {
        let args = seed_args();
        ResourceManager {
            resources: HashMap::new(),
            preprocessor: "/usr/bin/cpp".to_string(),
            logger: log::Logger::from(&args),
            args,
        }
    }
    fn get_resource_seeded_manager() -> Result<ResourceManager, Box<dyn Error>> {
        let mut manager = seed_resource_manager();
        manager.init();
        manager.resources = example_file_parsed().1;
        Ok(manager)
    }

    fn new_tmp_file(contents: &str) -> Result<(File, String), io::Error> {
        let mut tmp_dir = std::env::temp_dir();
        let filename = format!("{}.res", Uuid::new_v4());
        tmp_dir.push(filename);
        let mut file_handle = File::create(tmp_dir.clone())?;
        file_handle.write_all(contents.as_bytes())?;
        file_handle.flush()?;
        Ok((file_handle, tmp_dir.to_str().unwrap().to_string()))
    }

    #[test]
    fn create_resource_manager_obj() {
        let args = seed_args();
        let manager = ResourceManager::from_args(&args);
        let expected = ResourceManager {
            args: args.clone(),
            resources: HashMap::new(),
            logger: Logger::from(&args),
            preprocessor: args.cpp.unwrap_or(String::from("/usr/bin/cpp")),
        };
        assert_eq!(expected, manager);
    }

    #[test]
    fn parse() {
        let tmp = example_file_parsed();
        let manager = seed_resource_manager();
        let resources = manager.parse_config(tmp.0);
        assert_eq!(tmp.1, resources);
    }

    #[test]
    fn query() {
        let manager = get_resource_seeded_manager().unwrap();
        let expected = "lockscreen-timeout :\t10\nscreen-resolution :\t1920x1080";
        let query_result = manager.query("screen");
        assert_eq!(expected, query_result);
    }

    #[test]
    fn get_resource() {
        let manager = get_resource_seeded_manager().unwrap();
        let expected = "1920x1080";
        let actual = manager.get_resource("screen-resolution");
        assert_eq!(actual, expected);
    }

    #[test]
    fn load() -> Result<(), Box<dyn Error>> {
        let mut manager = get_resource_seeded_manager()?;
        let conf_str = "org: Regolith\nversion:0.1\n screen-resolution:1280x720";
        let mut actual = manager.resources.clone();
        let parsed_conf_str = manager.parse_config(conf_str);
        for (k, v) in parsed_conf_str {
            actual.entry(k).or_insert(v);
        }
        let (_, path) = new_tmp_file(conf_str)?;
        manager.load_from_file(&path, false, "/usr/bin/cpp", &[]);
        assert_eq!(manager.resources, actual);
        Ok(())
    }

    #[test]
    fn merge() -> Result<(), Box<dyn Error>> {
        let mut manager = get_resource_seeded_manager()?;
        let conf_str = "org: Regolith\nversion:0.1\n screen-resolution:1280x720";
        let mut actual = manager.resources.clone();
        let parsed_conf_str = manager.parse_config(conf_str);
        println!("{:#?}", parsed_conf_str);
        for (k, v) in parsed_conf_str {
            actual.insert(k, v);
        }
        let (_, path) = new_tmp_file(conf_str)?;
        manager.merge_from_file(&path, false, "/usr/bin/cpp", &[]);
        assert_eq!(manager.resources, actual);
        Ok(())
    }

    /// Creates a directory whose path contains one space, holding a resource
    /// file that includes one file from an include directory and one file
    /// next to it, and uses the macro NAME.
    /// Returns (resource file, include directory, resource file contents).
    fn new_spaced_tree() -> Result<(String, String, String), io::Error> {
        let mut root: PathBuf = std::env::temp_dir();
        root.push(format!("trawl-{} home", Uuid::new_v4()));
        let include_dir = root.join("Xresources.d");
        fs::create_dir_all(&include_dir)?;
        fs::write(include_dir.join("extra"), "extra.key: 1\n")?;
        fs::write(root.join("local"), "local.key: 2\n")?;
        let contents = "#include <extra>\n#include \"local\"\nname.key: NAME\n";
        let file = root.join("Xresources");
        fs::write(&file, contents)?;
        Ok((
            file.to_str().unwrap().to_string(),
            include_dir.to_str().unwrap().to_string(),
            contents.to_string(),
        ))
    }

    #[test]
    fn merge_with_spaces_in_cpp_args() -> Result<(), Box<dyn Error>> {
        let mut manager = seed_resource_manager();
        let (file, include_dir, contents) = new_spaced_tree()?;
        let args: Vec<String> = ["-I", &include_dir, "-D", "NAME=a b"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        manager.merge_from_file(&file, false, "/usr/bin/cpp", &args);
        assert_eq!(fs::read_to_string(&file)?, contents);
        assert_eq!(manager.get_resource("extra.key"), "1");
        assert_eq!(manager.get_resource("local.key"), "2");
        assert_eq!(manager.get_resource("name.key"), "a b");
        Ok(())
    }

    #[test]
    fn load_with_spaces_in_cpp_args() -> Result<(), Box<dyn Error>> {
        let mut manager = seed_resource_manager();
        let (file, include_dir, contents) = new_spaced_tree()?;
        let args = vec![String::from("-I"), include_dir, String::from("-DNAME=x")];
        manager.load_from_file(&file, false, "/usr/bin/cpp", &args);
        assert_eq!(fs::read_to_string(&file)?, contents);
        assert_eq!(manager.get_resource("extra.key"), "1");
        assert_eq!(manager.get_resource("local.key"), "2");
        assert_eq!(manager.get_resource("name.key"), "x");
        Ok(())
    }

    #[test]
    fn legacy_args_with_spaces_keep_file() -> Result<(), Box<dyn Error>> {
        // The whitespace-separated string of LoadCpp/MergeCpp can't carry an
        // argument with a space; the preprocessor then sees an extra operand,
        // which cpp would take as its input, and the next one as its output.
        // That must never touch the resource file.
        let mut manager = seed_resource_manager();
        let (file, include_dir, contents) = new_spaced_tree()?;
        let legacy = format!("-I {include_dir}");
        manager.merge_from_file(&file, false, "/usr/bin/cpp", &split_legacy_args(&legacy));
        assert_eq!(fs::read_to_string(&file)?, contents);
        manager.load_from_file(
            &file,
            false,
            "/usr/bin/cpp",
            &split_legacy_args("-D NAME=a b"),
        );
        assert_eq!(fs::read_to_string(&file)?, contents);
        Ok(())
    }

    #[test]
    fn split_legacy_cpp_args() {
        assert_eq!(split_legacy_args(""), Vec::<String>::new());
        assert_eq!(split_legacy_args("  "), Vec::<String>::new());
        assert_eq!(
            split_legacy_args("-I /usr/share -D MODKEY=Mod4  -D SWAY"),
            vec!["-I", "/usr/share", "-D", "MODKEY=Mod4", "-D", "SWAY"]
        );
    }

    #[test]
    fn check_valid_key() {
        let invalid_keys = [
            "hello'", "world\\", "`bad`", "string%", "comma,", "#hash", "no space",
        ];
        let valid_keys = ["hello-world", "good_day", "sway.output", "normal"];
        let manager = seed_resource_manager();
        for key in invalid_keys {
            println!("{key}");
            assert_eq!(manager.check_valid_key(key), false);
        }
        for key in valid_keys {
            println!("{key}");
            assert_eq!(manager.check_valid_key(key), true);
        }
    }

    #[test]
    fn remove_all() {
        let mut manager = get_resource_seeded_manager().unwrap();
        manager.handle_remove_all();
        assert_eq!(manager.resources, HashMap::<String, String>::new());
    }

    #[test]
    fn remove_one() {
        let mut manager = get_resource_seeded_manager().unwrap();
        let (_, mut initial) = example_file_parsed();
        let expected_entry = initial.remove_entry("home_dir");
        let removed_entry = manager.handle_remove_one("home_dir");
        assert_eq!(expected_entry, removed_entry);
        assert_eq!(initial, manager.resources);
    }
}

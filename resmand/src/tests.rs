
#[cfg(test)]
pub mod unit_test {
    use crate::*;
    use uuid::Uuid;
    use std::{io::{self, Write}, fs::File};

    fn example_file_parsed() -> (&'static str, HashMap<String, String>) {
        const file_contents: &str = "\
home_dir:/home
    key_one:val_one\t
invalid-line's#!\\
lockscreen-timeout:10     
\tscreen-resolution:    1920x1080  
valid: \"line\"
\"hello\' : invalid
 /home: invalid
 string\\: invalid
        ";
        let expected_map:HashMap<_,_> = [
            ("home_dir", "/home"), 
            ("key_one", "val_one"), 
            ("lockscreen-timeout", "10"),
            ("valid", "\"line\""),
            ("screen-resolution", "1920x1080")
        ].iter()
            .map(|&(k,v)| (k.to_string(),v.to_string()))
            .collect();
        (file_contents, expected_map)
    }

    fn seed_args() -> CliArgs {
        CliArgs{
            load: None,
            cpp: None,
            nocpp: false,
            filename: None,
            debug: true,
            verbose:false,
        }
    }
    pub fn seed_resource_manager() -> ResourceManager {
        let args = seed_args();
        ResourceManager {
            resources: HashMap::new(),
            preprocessor: "/usr/bin/cpp".to_string(),
            logger: log::Logger::from(&args),
            args
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
    fn create_resource_manager_obj () {
        let args = seed_args();
        let manager = ResourceManager::from_args(&args);
        let expected = ResourceManager {
            args: args.clone(),
            resources: HashMap::new(),
            logger: Logger::from(&args),
            preprocessor: args.cpp.unwrap_or(String::from("/usr/bin/cpp"))
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
    fn load() -> Result<(), Box<dyn Error>>{
        let mut manager = get_resource_seeded_manager()?;
        let conf_str = "org: Regolith\nversion:0.1\n screen-resolution:1280x720";
        let mut actual = manager.resources.clone();
        let parsed_conf_str = manager.parse_config(conf_str);
        for (k, v) in parsed_conf_str {
            actual.entry(k).or_insert(v);
        }
        let (_, path) = new_tmp_file(conf_str)?;
        manager.load_from_file(&path);
        assert_eq!(manager.resources, actual);
        Ok(())
    }

    #[test]
    fn merge() -> Result<(), Box<dyn Error>>{
        let mut manager = get_resource_seeded_manager()?;
        let conf_str = "org: Regolith\nversion:0.1\n screen-resolution:1280x720";
        let mut actual = manager.resources.clone();
        let parsed_conf_str = manager.parse_config(conf_str);
        println!("{:#?}", parsed_conf_str);
        for (k, v) in parsed_conf_str {
            actual.insert(k, v);
        }
        let (_, path) = new_tmp_file(conf_str)?;
        manager.merge_from_file(&path);
        assert_eq!(manager.resources, actual);
        Ok(())
    }

    #[test]
    fn check_valid_key() {
        let invalid_keys = ["hello'", "world\\", "`bad`", "string%", "comma,", "#hash", "no space"];
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

}

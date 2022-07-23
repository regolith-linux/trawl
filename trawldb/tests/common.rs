use io::Write;
use std::collections::HashMap;
use std::{fs::File, io};
use trawldb::{parser::CliArgs, Client};
use uuid::Uuid;

fn new_tmp_file(contents: &str) -> Result<(File, String), io::Error> {
    let mut tmp_dir = std::env::temp_dir();
    let filename = format!("{}.res", Uuid::new_v4());
    tmp_dir.push(filename);
    let mut file_handle = File::create(tmp_dir.clone())?;
    file_handle.write_all(contents.as_bytes())?;
    file_handle.flush()?;
    Ok((file_handle, tmp_dir.to_str().unwrap().to_string()))
}

fn get_config_str() -> (&'static str, HashMap<String, String>) {
    let config_str = "home_dir:/home
    key_one:val_one\t
    invalid-line's#!
    lockscreen-timeout:10     
    \tscreen-resolution:    1920x1080  
    valid: \"line\"
    \"hello\' : invalid
    /home: invalid
    string\\: invalid";
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
    (config_str, expected_map)
}

fn get_file() -> (String, HashMap<String, String>) {
    let (config_str, resources) = get_config_str();
    let (_, path_str) = new_tmp_file(config_str).unwrap();
    (path_str, resources)
}

pub async fn get_resources_prop() -> zbus::Result<HashMap<String, String>> {
    let client = Client::new().await.unwrap();
    client.proxy().resources().await
}

pub async fn arg_filename_default() -> HashMap<String, String> {
    let (file_path, resources) = get_file();
    let args = CliArgs {
        nocpp: false,
        filename: None,
        cpp: None,
        load: Some(file_path),
        merge: None,
        edit: None,
        backup: None,
        get: None,
        query: None,
        remove: false,
        define: vec![],
        include: vec![],
    };
    let client = Client::new().await.unwrap();
    client.run(&args).await.unwrap();
    resources
}

pub async fn arg_load() -> HashMap<String, String> {
    let mut curr_resources = arg_filename_default().await;
    let file_path = new_tmp_file("key_1: val2").unwrap().1;
    curr_resources.insert(String::from("key_1"), String::from("val2"));

    let args = CliArgs {
        nocpp: false,
        filename: None,
        cpp: None,
        load: Some(file_path),
        merge: None,
        edit: None,
        backup: None,
        get: None,
        query: None,
        remove: false,
        define: vec![],
        include: vec![],
    };
    let client = Client::new().await.unwrap();
    client.run(&args).await.unwrap();
    curr_resources
}

pub async fn arg_merge() -> HashMap<String, String> {
    let mut curr_resources = arg_filename_default().await;
    let file = new_tmp_file("key_1: val2").unwrap().1;
    curr_resources.insert(String::from("key_1"), String::from("val2"));
    let args = CliArgs {
        nocpp: false,
        filename: None,
        cpp: None,
        load: None,
        merge: Some(file),
        edit: None,
        backup: None,
        get: None,
        query: None,
        remove: false,
        define: vec![],
        include: vec![],
    };
    let client = Client::new().await.unwrap();
    client.run(&args).await.unwrap();
    curr_resources
}

pub async fn arg_query() -> (String, String) {
    let _ = arg_filename_default().await;
    let q = "screen".to_string();
    let expected = "lockscreen-timeout :\t10\nscreen-resolution :\t1920x1080";
    (q, expected.to_string())
}

pub async fn query_all_result() -> String {
    let _ = arg_filename_default().await;
    let client = Client::new().await.unwrap();
    let expected = client.proxy().query("").await.unwrap();
    expected
}

pub async fn arg_edit(bak: Option<String>) -> (String, String, String) {
    let _ = arg_filename_default().await;
    let (_, path) = new_tmp_file("hello world").unwrap();
    let args = CliArgs {
        nocpp: false,
        filename: None,
        cpp: None,
        load: None,
        merge: None,
        edit: Some(path.clone()),
        backup: bak.clone(),
        get: None,
        query: None,
        remove: false,
        define: vec![],
        include: vec![],
    };
    let client = Client::new().await.unwrap();
    client.run(&args).await.unwrap();
    (
        path.clone(),
        path + &bak.unwrap_or(String::from(".bak")),
        "hello world".to_string(),
    )
}

pub async fn clear_resources() {
    let client = Client::new().await.unwrap();
    client.proxy().remove_all().await.unwrap();
}

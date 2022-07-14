//! **Note** 1: Run the config daemon `trawld` before running the tests
//! **Note** 2: Run the tests sequentially. 

mod common;

use std::{fs, collections::HashMap};

use trawldb::Client;
use common::*;

#[tokio::test]
async fn filename() {
    let expected = arg_filename_default().await;
    let curr_resources = get_resources_prop().await.unwrap();
    clear_resources().await;
    assert_eq!(expected, curr_resources);
}

#[tokio::test]
async fn merge() {
    let expected = arg_merge().await;
    let curr_resources = get_resources_prop().await.unwrap();
    clear_resources().await;
    assert_eq!(expected, curr_resources);
}

#[tokio::test]
async fn load() {
    let expected = arg_load().await;
    let curr_resources = get_resources_prop().await.unwrap();
    clear_resources().await;
    assert_eq!(expected, curr_resources);

}

#[tokio::test]
async fn query() {
    arg_filename_default().await;
    let (q, expected) = arg_query().await;
    let client = Client::new().await.unwrap();
    let actual_result = client.proxy().query(&q).await.unwrap();
    clear_resources().await;
    assert_eq!(expected, actual_result);
}

#[tokio::test]
async fn query_all() {
    arg_filename_default().await;
    let expected = query_all_result().await;
    let client = Client::new().await.unwrap();
    let actual_result = client.proxy().query("").await.unwrap();
    clear_resources().await;
    assert_eq!(expected, actual_result);
}

#[tokio::test]
async fn edit() {
    arg_filename_default().await;
    let client = Client::new().await.unwrap();
    let expected = client.proxy().query("").await.unwrap();
    let (path, bak, bak_expected) = arg_edit(None).await;
    let contents = fs::read_to_string(&path).unwrap();
    assert_eq!(expected, contents);
    let bak_content = fs::read_to_string(&bak).unwrap();
    assert_eq!(bak_expected, bak_content);
    clear_resources().await;
}

#[tokio::test]
async fn clear() {
    arg_filename_default().await;
    clear_resources().await;
    let expected = HashMap::<String, String>::new();
    let actual = get_resources_prop().await.unwrap();
    assert_eq!(expected, actual);
}

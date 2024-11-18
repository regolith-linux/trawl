use anyhow::{anyhow, Result};
use trawldb::Client;
pub async fn rescat(res_name: &str, default: Option<String>) -> Result<String> {
    let Ok(client) = Client::new().await else {
        return Err(anyhow!("Failed to connect to trawld"));
    };
    let resource = client.proxy().get_resource(res_name).await?;
    if resource == String::new() {
        default.ok_or(anyhow!("No default value provided"))
    } else {
        Ok(resource)
    }
}


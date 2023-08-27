use std::error::Error;
use string_error::into_err;
use trawldb::Client;
pub async fn rescat(res_name: &str, default: Option<String>) -> Result<String, Box<dyn Error>> {
    let client = Client::new().await?;
    let resource = client.proxy().get_resource(res_name).await?;
    if resource == String::new() {
        let no_default_err = String::from("No default value provided");
        default.ok_or(into_err(no_default_err))
    } else {
        Ok(resource)
    }
}


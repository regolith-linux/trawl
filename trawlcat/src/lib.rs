use anyhow::{anyhow, Result};
use trawldb::Client;

/// Like xrescat: a resource that exists is printed, even with an empty
/// value, with one pair of surrounding double quotes removed; the default
/// is used only for a missing resource.
pub async fn rescat(res_name: &str, default: Option<String>) -> Result<String> {
    let Ok(client) = Client::new().await else {
        return Err(anyhow!("Failed to connect to trawld"));
    };
    let (found, resource) = client.proxy().lookup_resource(res_name).await?;
    if found {
        Ok(strip_quotes(&resource).to_string())
    } else {
        default.ok_or(anyhow!("No default value provided"))
    }
}

/// Removes one pair of surrounding double quotes
fn strip_quotes(value: &str) -> &str {
    match value.strip_prefix('"').and_then(|v| v.strip_suffix('"')) {
        Some(unquoted) => unquoted,
        None => value,
    }
}

#[cfg(test)]
mod tests {
    use super::strip_quotes;

    #[test]
    fn strips_one_pair_of_quotes() {
        assert_eq!(strip_quotes("\"quoted\""), "quoted");
        assert_eq!(strip_quotes("\"\"nested\"\""), "\"nested\"");
        assert_eq!(strip_quotes("\"\""), "");
        assert_eq!(strip_quotes("\""), "\"");
        assert_eq!(strip_quotes("\"half"), "\"half");
        assert_eq!(strip_quotes("a\"b"), "a\"b");
    }
}

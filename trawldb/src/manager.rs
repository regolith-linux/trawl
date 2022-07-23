use std::collections::HashMap;
use zbus::{dbus_proxy, Result};

#[dbus_proxy(
    default_service = "org.regolith.Trawl",
    default_path = "/org/regolith/Trawl",
    interface = "org.regolith.trawl1"
)]
#[dbus_proxy]
pub trait ResourceManager {
    fn load(&self, path: &str, nocpp: bool) -> Result<()>;
    fn merge(&self, path: &str, nocpp: bool) -> Result<()>;
    fn load_cpp(&self, path: &str, cpp: &str, args: &str) -> Result<()>;
    fn merge_cpp(&self, path: &str, cpp: &str, args: &str) -> Result<()>;
    fn query(&self, q: &str) -> Result<String>;
    fn get_resource(&self, key: &str) -> Result<String>;
    fn set_resource(&self, key: String, value: String) -> Result<()>;
    fn add_resource(&self, key: String, value: String) -> Result<()>;
    fn quit(&self) -> Result<()>;
    fn remove_one(&self, key: &str) -> Result<()>;
    fn remove_all(&self) -> Result<()>;

    #[dbus_proxy(property)]
    fn resources(&self) -> Result<HashMap<String, String>>;
}

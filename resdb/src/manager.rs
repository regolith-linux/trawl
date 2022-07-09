use std::collections::HashMap;
use zbus::{Result, dbus_proxy};

#[dbus_proxy(
    default_service = "org.regolith.ConfigMgr",
    default_path = "/org/regolith/ConfigMgr",
    interface = "org.regolith.config1"
)]
#[dbus_proxy]
pub trait ResourceManager {
    fn load(&self, path: &str, nocpp: bool) -> Result<()>;
    fn merge(&self, path: &str, nocpp: bool) -> Result<()>;
    fn query(&self, q: &str) -> Result<String>;
    fn get_resource(&self, key: &str) -> Result<String>;
    fn set_resource(&self, key: String, value: String) -> Result<()>;
    fn add_resource(&self, key: String, value: String) -> Result<()>;

    #[dbus_proxy(property)]
    fn resource(&self) -> Result<HashMap<String, String>>;

    #[dbus_proxy(signal)]
    fn resources_changed(&self) -> Result<()>;
}

//! Trait for registering config format plugins without a dependency cycle.
pub trait ConfigFormatRegistrar {
    /// Register a config format plugin.
    fn register_config_plugin(&mut self, plugin: std::sync::Arc<dyn crate::ConfigFormatPlugin>);
}

//! Built-in language plugins (feature-gated via `languages.toml` / Cargo features)

#[cfg(feature = "lang-rust")]
pub mod rust;
#[cfg(feature = "lang-python")]
pub mod python;
#[cfg(feature = "lang-typescript")]
pub mod typescript;
#[cfg(feature = "lang-javascript")]
pub mod javascript;
#[cfg(feature = "lang-go")]
pub mod go;
#[cfg(feature = "lang-java")]
pub mod java;

#[cfg(feature = "lang-rust")]
pub use rust::RustPlugin;
#[cfg(feature = "lang-python")]
pub use python::PythonPlugin;
#[cfg(feature = "lang-typescript")]
pub use typescript::TypeScriptPlugin;
#[cfg(feature = "lang-javascript")]
pub use javascript::JavaScriptPlugin;
#[cfg(feature = "lang-go")]
pub use go::GoPlugin;
#[cfg(feature = "lang-java")]
pub use java::JavaPlugin;

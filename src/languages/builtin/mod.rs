//! Built-in language plugins

pub mod rust;
pub mod python;
pub mod typescript;
pub mod javascript;
pub mod go;

pub use go::GoPlugin;
pub use javascript::JavaScriptPlugin;
pub use python::PythonPlugin;
pub use rust::RustPlugin;
pub use typescript::TypeScriptPlugin;

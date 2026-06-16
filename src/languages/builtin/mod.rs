//! Built-in language plugins

pub mod rust;
pub mod python;
pub mod typescript;
pub mod javascript;
pub mod go;
pub mod java;
pub mod kotlin;
pub mod csharp;

pub use csharp::CSharpPlugin;
pub use go::GoPlugin;
pub use java::JavaPlugin;
pub use javascript::JavaScriptPlugin;
pub use kotlin::KotlinPlugin;
pub use python::PythonPlugin;
pub use rust::RustPlugin;
pub use typescript::TypeScriptPlugin;

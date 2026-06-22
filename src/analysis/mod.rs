//! Graph analysis algorithms (monolith re-exports + IaC from lang crates)

pub use rbuilder_analysis::*;

#[cfg(feature = "iac-langs")]
pub mod ansible_roles {
    //! Ansible role dependency analysis.
    pub use rbuilder_lang_ansible::analysis::*;
}

#[cfg(feature = "iac-langs")]
pub mod chef_cookbooks {
    //! Chef cookbook dependency analysis.
    pub use rbuilder_lang_chef::analysis::*;
}

#[cfg(feature = "iac-langs")]
pub mod puppet_modules {
    //! Puppet module dependency analysis.
    pub use rbuilder_lang_puppet::analysis::*;
}

#[cfg(feature = "iac-langs")]
pub use ansible_roles::{RoleDependencyAnalyzer, RoleDependencyGraph, RoleNode};
#[cfg(feature = "iac-langs")]
pub use chef_cookbooks::{CookbookDependencyAnalyzer, CookbookDependencyGraph, CookbookNode};
#[cfg(feature = "iac-langs")]
pub use puppet_modules::{ModuleDependencyAnalyzer, ModuleDependencyGraph, ModuleNode};

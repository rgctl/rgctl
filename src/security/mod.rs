//! Security analysis (monolith re-exports + IaC from lang crates)

pub use rbuilder_security::*;

#[cfg(feature = "iac-langs")]
pub mod ansible {
    //! Ansible security scanning.
    pub use rbuilder_lang_ansible::security::*;
}

#[cfg(feature = "iac-langs")]
pub mod chef {
    //! Chef security scanning.
    pub use rbuilder_lang_chef::security::*;
}

#[cfg(feature = "iac-langs")]
pub mod puppet {
    //! Puppet security scanning.
    pub use rbuilder_lang_puppet::security::*;
}

#[cfg(feature = "iac-langs")]
pub use ansible::{AnsibleSecurityFinding, AnsibleSecurityScanner, AnsibleSeverity};
#[cfg(feature = "iac-langs")]
pub use chef::{ChefSecurityFinding, ChefSecurityScanner, ChefSeverity};
#[cfg(feature = "iac-langs")]
pub use puppet::{PuppetSecurityFinding, PuppetSecurityScanner, PuppetSeverity};

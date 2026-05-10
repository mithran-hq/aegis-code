use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VersionInfo {
    pub release_version: &'static str,
    pub upstream_repository: &'static str,
    pub upstream_base: &'static str,
    pub source_revision: &'static str,
}

pub const RELEASE_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const UPSTREAM_REPOSITORY: &str = env!("AEGIS_UPSTREAM_REPOSITORY");
pub const UPSTREAM_BASE: &str = env!("AEGIS_UPSTREAM_BASE");
pub const SOURCE_REVISION: &str = env!("AEGIS_SOURCE_REVISION");
pub const CLAP_VERSION: &str = concat!(
    env!("CARGO_PKG_VERSION"),
    " (upstream ",
    env!("AEGIS_UPSTREAM_BASE"),
    ", source ",
    env!("AEGIS_SOURCE_REVISION"),
    ")"
);

pub fn info() -> VersionInfo {
    VersionInfo {
        release_version: RELEASE_VERSION,
        upstream_repository: UPSTREAM_REPOSITORY,
        upstream_base: UPSTREAM_BASE,
        source_revision: SOURCE_REVISION,
    }
}

pub fn clap_version() -> &'static str {
    CLAP_VERSION
}

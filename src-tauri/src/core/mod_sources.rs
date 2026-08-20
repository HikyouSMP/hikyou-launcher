//! Mod source abstraction.
//!
//! The UI still exposes Modrinth-first commands for compatibility, but command
//! handlers should go through this module so future providers can be added
//! without threading provider-specific calls through the app.

use std::path::Path;

use super::mods::{ModFile, ModSearchResult, ModpackInstallResult, ModpackVersionInfo};
use futures::future::BoxFuture;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModSource {
    Modrinth,
}

trait ModProvider {
    fn search_mods<'a>(
        &'a self,
        query: ModSearchQuery<'a>,
    ) -> BoxFuture<'a, Result<Vec<ModSearchResult>, String>>;

    fn install_mod<'a>(
        &'a self,
        request: ModInstallRequest<'a>,
    ) -> BoxFuture<'a, Result<Vec<ModFile>, String>>;

    fn search_modpacks<'a>(
        &'a self,
        query: &'a str,
        mc_version: &'a str,
    ) -> BoxFuture<'a, Result<Vec<ModSearchResult>, String>>;

    fn get_modpack_versions<'a>(
        &'a self,
        project_id: &'a str,
    ) -> BoxFuture<'a, Result<Vec<ModpackVersionInfo>, String>>;

    fn install_modpack<'a>(
        &'a self,
        request: ModpackInstallRequest<'a>,
    ) -> BoxFuture<'a, Result<ModpackInstallResult, String>>;
}

struct ModrinthProvider;

impl ModProvider for ModrinthProvider {
    fn search_mods<'a>(
        &'a self,
        query: ModSearchQuery<'a>,
    ) -> BoxFuture<'a, Result<Vec<ModSearchResult>, String>> {
        Box::pin(super::mods::search_modrinth(
            query.query,
            query.loader,
            query.mc_version,
        ))
    }

    fn install_mod<'a>(
        &'a self,
        request: ModInstallRequest<'a>,
    ) -> BoxFuture<'a, Result<Vec<ModFile>, String>> {
        Box::pin(super::mods::install_modrinth_mod(
            request.game_dir,
            request.project_id,
            request.mc_version,
            request.loader,
            request.display_name,
            request.icon_url,
        ))
    }

    fn search_modpacks<'a>(
        &'a self,
        query: &'a str,
        mc_version: &'a str,
    ) -> BoxFuture<'a, Result<Vec<ModSearchResult>, String>> {
        Box::pin(super::mods::search_modpacks(query, mc_version))
    }

    fn get_modpack_versions<'a>(
        &'a self,
        project_id: &'a str,
    ) -> BoxFuture<'a, Result<Vec<ModpackVersionInfo>, String>> {
        Box::pin(super::mods::get_modpack_versions(project_id))
    }

    fn install_modpack<'a>(
        &'a self,
        request: ModpackInstallRequest<'a>,
    ) -> BoxFuture<'a, Result<ModpackInstallResult, String>> {
        Box::pin(super::mods::install_modpack(
            request.game_dir,
            request.project_id,
            request.mc_version,
            request.version_id,
        ))
    }
}

static MODRINTH_PROVIDER: ModrinthProvider = ModrinthProvider;

fn provider_for(source: ModSource) -> &'static dyn ModProvider {
    match source {
        ModSource::Modrinth => &MODRINTH_PROVIDER,
    }
}

pub struct ModSearchQuery<'a> {
    pub query: &'a str,
    pub loader: &'a str,
    pub mc_version: &'a str,
}

pub struct ModInstallRequest<'a> {
    pub game_dir: &'a Path,
    pub project_id: &'a str,
    pub mc_version: &'a str,
    pub loader: &'a str,
    pub display_name: Option<String>,
    pub icon_url: Option<String>,
}

pub struct ModpackInstallRequest<'a> {
    pub game_dir: &'a Path,
    pub project_id: &'a str,
    pub mc_version: &'a str,
    pub version_id: Option<&'a str>,
}

pub async fn search_mods(
    source: ModSource,
    query: ModSearchQuery<'_>,
) -> Result<Vec<ModSearchResult>, String> {
    provider_for(source).search_mods(query).await
}

pub async fn install_mod(
    source: ModSource,
    request: ModInstallRequest<'_>,
) -> Result<Vec<ModFile>, String> {
    provider_for(source).install_mod(request).await
}

pub async fn search_modpacks(
    source: ModSource,
    query: &str,
    mc_version: &str,
) -> Result<Vec<ModSearchResult>, String> {
    provider_for(source)
        .search_modpacks(query, mc_version)
        .await
}

pub async fn get_modpack_versions(
    source: ModSource,
    project_id: &str,
) -> Result<Vec<ModpackVersionInfo>, String> {
    provider_for(source).get_modpack_versions(project_id).await
}

pub async fn install_modpack(
    source: ModSource,
    request: ModpackInstallRequest<'_>,
) -> Result<ModpackInstallResult, String> {
    provider_for(source).install_modpack(request).await
}

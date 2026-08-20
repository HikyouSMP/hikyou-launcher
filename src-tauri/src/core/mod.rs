//! ランチャーコアモジュール
//! パス解決、ゲーム起動ロジックを含む。

pub mod assets;
pub mod cache;
mod crash_diagnosis;
mod crash_messages;
pub mod crash_parser;
mod crash_rule_db;
mod crash_rules;
pub mod downloader;
pub mod fabric;
pub mod forge;
pub mod icon_cache;
pub mod java;
pub mod launcher;
mod launcher_args;
mod launcher_files;
pub mod launcher_state;
pub mod manifest;
mod mod_files;
mod mod_installer;
mod mod_metadata;
pub mod mod_recommendations;
pub mod mod_sources;
mod mod_sync_state;
pub mod modpacks;
mod modrinth_provider;
pub mod mods;
pub mod neoforge;
pub mod paths;
pub mod profile;
pub mod quilt;
pub mod running_processes;

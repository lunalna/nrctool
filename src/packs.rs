use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{Read, Write};
use std::path::Path;

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};

const MODRINTH_MAVEN: &str = "https://api.modrinth.com/maven";
const INSTALL_STATE: &str = ".nrctool-installed.json";

#[derive(Clone, Deserialize)]
pub struct PackConfig {
    packs: HashMap<String, Pack>,
    repositories: HashMap<String, String>,
}

#[derive(Clone, Deserialize)]
struct Pack {
    #[serde(rename = "displayName")]
    display_name: String,
    #[serde(rename = "inheritsFrom", default)]
    inherits_from: Option<Vec<String>>,
    #[serde(rename = "excludeMods", default)]
    exclude_mods: Option<Vec<String>>,
    #[serde(default)]
    mods: Vec<ModEntry>,
    #[serde(default)]
    listing: Listing,
}

#[derive(Clone, Default, Deserialize)]
struct Listing {
    #[serde(default)]
    category: String,
}

#[derive(Clone, Deserialize)]
struct ModEntry {
    id: String,
    source: ModSource,
    compatibility: HashMap<String, HashMap<String, CompatibilityTarget>>,
}

#[derive(Clone, Deserialize)]
struct ModSource {
    #[serde(rename = "type")]
    kind: String,
    #[serde(rename = "projectSlug")]
    project_slug: Option<String>,
    #[serde(rename = "repositoryRef")]
    repository_ref: Option<String>,
    #[serde(rename = "groupId")]
    group_id: Option<String>,
    #[serde(rename = "artifactId")]
    artifact_id: Option<String>,
}

#[derive(Clone, Deserialize)]
struct CompatibilityTarget {
    identifier: String,
    filename: Option<String>,
    source: Option<ModSource>,
}

#[derive(Clone)]
pub struct Branch {
    pub id: String,
    pub display_name: String,
}

pub enum InstallProgress {
    Progress {
        completed: usize,
        total: usize,
        filename: String,
    },
    Finished(usize),
    Failed(String),
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct InstalledPack {
    #[serde(default)]
    pub branch: String,
    #[serde(default)]
    pub game_version: String,
    #[serde(default)]
    pub loader: String,
    #[serde(default)]
    files: Vec<String>,
}

struct Download {
    url: String,
    filename: String,
}

impl PackConfig {
    pub fn fetch(url: &str) -> Result<Self, String> {
        Client::builder()
            .user_agent("nrctool/0.1")
            .build()
            .map_err(|error| error.to_string())?
            .get(url)
            .send()
            .and_then(|response| response.error_for_status())
            .map_err(|error| format!("Could not load NoRisk branches: {error}"))?
            .json()
            .map_err(|error| format!("Invalid NoRisk manifest: {error}"))
    }

    pub fn game_versions(&self) -> Vec<String> {
        let mut values = HashSet::new();
        for id in self.core_pack_ids() {
            if let Ok(mods) = self.resolve_mods(&id) {
                for entry in mods.values() {
                    values.extend(entry.compatibility.keys().cloned());
                }
            }
        }
        let mut values: Vec<_> = values.into_iter().collect();
        values.sort_by(|a, b| version_parts(b).cmp(&version_parts(a)));
        values
    }

    pub fn loaders_for(&self, game_version: &str) -> Vec<String> {
        let mut values = HashSet::new();
        for id in self.core_pack_ids() {
            if let Ok(mods) = self.resolve_mods(&id) {
                for entry in mods.values() {
                    if let Some(loaders) = entry.compatibility.get(game_version) {
                        values.extend(loaders.keys().cloned());
                    }
                }
            }
        }
        let mut values: Vec<_> = values.into_iter().collect();
        values.sort_by(|a, b| loader_order(a).cmp(&loader_order(b)));
        values
    }

    pub fn branches_for(&self, game_version: &str, loader: &str) -> Vec<Branch> {
        let mut values = Vec::new();
        for id in self.core_pack_ids() {
            let compatible = self.resolve_mods(&id).is_ok_and(|mods| {
                mods.values()
                    .any(|entry| compatible(entry, game_version, loader))
            });
            if compatible {
                let pack = &self.packs[&id];
                values.push(Branch {
                    id,
                    display_name: pack.display_name.clone(),
                });
            }
        }
        values.sort_by(|a, b| {
            branch_order(&a.id)
                .cmp(&branch_order(&b.id))
                .then(a.display_name.cmp(&b.display_name))
        });
        values
    }

    pub fn install(
        &self,
        branch: &str,
        game_version: &str,
        loader: &str,
        mods_folder: &Path,
        mut report: impl FnMut(InstallProgress),
    ) -> Result<usize, String> {
        let mods = self.resolve_mods(branch)?;
        let mut downloads = Vec::new();
        for entry in mods.values() {
            let Some(target) = entry
                .compatibility
                .get(game_version)
                .and_then(|loaders| loaders.get(loader))
            else {
                continue;
            };
            downloads.push(self.download_for(entry, target)?);
        }
        downloads.sort_by(|a, b| a.filename.cmp(&b.filename));

        for download in &downloads {
            validate_filename(&download.filename)?;
        }
        for pair in downloads.windows(2) {
            if pair[0].filename == pair[1].filename {
                return Err(format!(
                    "The pack contains duplicate filename '{}'",
                    pair[0].filename
                ));
            }
        }

        if downloads.is_empty() {
            return Err("The selected branch has no compatible mods".into());
        }

        fs::create_dir_all(mods_folder)
            .map_err(|error| format!("Could not create mods folder: {error}"))?;
        let staging = mods_folder.join(".nrctool-downloads");
        if staging.exists() {
            fs::remove_dir_all(&staging)
                .map_err(|error| format!("Could not clear temporary downloads: {error}"))?;
        }
        fs::create_dir(&staging)
            .map_err(|error| format!("Could not create temporary download folder: {error}"))?;

        let client = Client::builder()
            .user_agent("nrctool/0.1")
            .build()
            .map_err(|error| error.to_string())?;

        for (index, download) in downloads.iter().enumerate() {
            download_file(&client, download, &staging)?;
            report(InstallProgress::Progress {
                completed: index + 1,
                total: downloads.len(),
                filename: download.filename.clone(),
            });
        }

        let old_state = installed_pack(mods_folder).unwrap_or_default();
        let new_files: HashSet<_> = downloads
            .iter()
            .map(|item| item.filename.as_str())
            .collect();
        for filename in old_state.files {
            if validate_filename(&filename).is_err() {
                continue;
            }
            if !new_files.contains(filename.as_str()) {
                let path = mods_folder.join(&filename);
                if path.is_file() {
                    fs::remove_file(&path).map_err(|error| {
                        format!("Could not replace {}: {error}", path.display())
                    })?;
                }
            }
        }

        for download in &downloads {
            let destination = mods_folder.join(&download.filename);
            if destination.is_file() {
                fs::remove_file(&destination).map_err(|error| {
                    format!("Could not replace {}: {error}", destination.display())
                })?;
            }
            fs::rename(staging.join(&download.filename), destination)
                .map_err(|error| format!("Could not install {}: {error}", download.filename))?;
        }
        fs::remove_dir(&staging).ok();

        let state = InstalledPack {
            branch: branch.to_string(),
            game_version: game_version.to_string(),
            loader: loader.to_string(),
            files: downloads.iter().map(|item| item.filename.clone()).collect(),
        };
        let state_json = serde_json::to_vec_pretty(&state).map_err(|error| error.to_string())?;
        fs::write(mods_folder.join(INSTALL_STATE), state_json)
            .map_err(|error| format!("Could not save installation state: {error}"))?;

        Ok(downloads.len())
    }

    pub fn detect_install(&self, mods_folder: &Path) -> Option<InstalledPack> {
        if let Some(state) = installed_pack(mods_folder) {
            if !state.files.is_empty() {
                if !state.branch.is_empty() {
                    return Some(state);
                }
                if let Some(detected) = self.detect_from_files(mods_folder) {
                    return Some(detected);
                }
                return Some(state);
            }
        }
        self.detect_from_files(mods_folder)
    }

    fn detect_from_files(&self, mods_folder: &Path) -> Option<InstalledPack> {
        let existing: HashSet<String> = fs::read_dir(mods_folder)
            .ok()?
            .flatten()
            .filter_map(|entry| entry.file_name().into_string().ok())
            .collect();
        let mut best: Option<InstalledPack> = None;

        for branch in self.core_pack_ids() {
            let Ok(mods) = self.resolve_mods(&branch) else {
                continue;
            };
            for game_version in self.game_versions() {
                for loader in self.loaders_for(&game_version) {
                    let files: Vec<String> = mods
                        .values()
                        .filter_map(|entry| {
                            let target = entry.compatibility.get(&game_version)?.get(&loader)?;
                            self.download_for(entry, target)
                                .ok()
                                .map(|item| item.filename)
                        })
                        .collect();
                    if files.is_empty() || !files.iter().all(|file| existing.contains(file)) {
                        continue;
                    }
                    if best
                        .as_ref()
                        .is_none_or(|current| files.len() > current.files.len())
                    {
                        best = Some(InstalledPack {
                            branch: branch.clone(),
                            game_version: game_version.clone(),
                            loader: loader.clone(),
                            files,
                        });
                    }
                }
            }
        }
        best
    }

    fn core_pack_ids(&self) -> Vec<String> {
        self.packs
            .iter()
            .filter(|(_, pack)| pack.listing.category != "partner")
            .map(|(id, _)| id.clone())
            .collect()
    }

    fn resolve_mods(&self, pack_id: &str) -> Result<HashMap<String, ModEntry>, String> {
        self.resolve_mods_inner(pack_id, &mut HashSet::new())
    }

    fn resolve_mods_inner(
        &self,
        pack_id: &str,
        visiting: &mut HashSet<String>,
    ) -> Result<HashMap<String, ModEntry>, String> {
        if !visiting.insert(pack_id.to_string()) {
            return Err(format!("Circular pack inheritance at {pack_id}"));
        }
        let pack = self
            .packs
            .get(pack_id)
            .ok_or_else(|| format!("Unknown NoRisk branch: {pack_id}"))?;
        let mut resolved = HashMap::new();
        if let Some(parents) = &pack.inherits_from {
            for parent in parents {
                resolved.extend(self.resolve_mods_inner(parent, visiting)?);
            }
        }
        for entry in &pack.mods {
            resolved.insert(entry.id.clone(), entry.clone());
        }
        if let Some(excluded_mods) = &pack.exclude_mods {
            for excluded in excluded_mods {
                resolved.remove(excluded);
            }
        }
        visiting.remove(pack_id);
        Ok(resolved)
    }

    fn download_for(
        &self,
        entry: &ModEntry,
        target: &CompatibilityTarget,
    ) -> Result<Download, String> {
        let source = target.source.as_ref().unwrap_or(&entry.source);
        match source.kind.as_str() {
            "modrinth" => {
                let slug = required(&source.project_slug, "projectSlug", &entry.id)?;
                let filename = target
                    .filename
                    .clone()
                    .unwrap_or_else(|| format!("{slug}-{}.jar", target.identifier));
                let url = maven_url(
                    MODRINTH_MAVEN,
                    "maven.modrinth",
                    slug,
                    &target.identifier,
                    &filename,
                );
                Ok(Download { url, filename })
            }
            "maven" => {
                let repository_ref = required(&source.repository_ref, "repositoryRef", &entry.id)?;
                let repository = self.repositories.get(repository_ref).ok_or_else(|| {
                    format!("Unknown repository '{repository_ref}' for {}", entry.id)
                })?;
                let group = required(&source.group_id, "groupId", &entry.id)?;
                let artifact = required(&source.artifact_id, "artifactId", &entry.id)?;
                let filename = target
                    .filename
                    .clone()
                    .unwrap_or_else(|| format!("{artifact}-{}.jar", target.identifier));
                let url = maven_url(repository, group, artifact, &target.identifier, &filename);
                Ok(Download { url, filename })
            }
            "url" => {
                let filename = target
                    .filename
                    .clone()
                    .ok_or_else(|| format!("Direct URL mod '{}' has no filename", entry.id))?;
                Ok(Download {
                    url: target.identifier.clone(),
                    filename,
                })
            }
            kind => Err(format!("Unsupported source type '{kind}' for {}", entry.id)),
        }
    }
}

fn compatible(entry: &ModEntry, game_version: &str, loader: &str) -> bool {
    entry
        .compatibility
        .get(game_version)
        .is_some_and(|loaders| loaders.contains_key(loader))
}

fn required<'a>(value: &'a Option<String>, field: &str, mod_id: &str) -> Result<&'a str, String> {
    value
        .as_deref()
        .ok_or_else(|| format!("Mod '{mod_id}' is missing {field}"))
}

fn validate_filename(filename: &str) -> Result<(), String> {
    let path = Path::new(filename);
    let is_single_component = path.components().count() == 1 && path.file_name().is_some();
    if !is_single_component || !filename.to_ascii_lowercase().ends_with(".jar") {
        return Err(format!("Unsafe mod filename '{filename}'"));
    }
    Ok(())
}

fn maven_url(
    repository: &str,
    group: &str,
    artifact: &str,
    version: &str,
    filename: &str,
) -> String {
    format!(
        "{}/{}/{}/{}/{}",
        repository.trim_end_matches('/'),
        group.replace('.', "/"),
        artifact,
        version,
        filename
    )
}

fn download_file(client: &Client, download: &Download, folder: &Path) -> Result<(), String> {
    let mut response = client
        .get(&download.url)
        .send()
        .and_then(|response| response.error_for_status())
        .map_err(|error| format!("Failed to download {}: {error}", download.filename))?;
    let path = folder.join(&download.filename);
    let mut file = fs::File::create(&path)
        .map_err(|error| format!("Could not create {}: {error}", path.display()))?;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = response
            .read(&mut buffer)
            .map_err(|error| format!("Failed while downloading {}: {error}", download.filename))?;
        if count == 0 {
            break;
        }
        file.write_all(&buffer[..count])
            .map_err(|error| format!("Could not write {}: {error}", path.display()))?;
    }
    drop(file);

    let mut downloaded = fs::File::open(&path)
        .map_err(|error| format!("Could not verify {}: {error}", path.display()))?;
    let mut header = [0_u8; 2];
    if downloaded.read_exact(&mut header).is_err() || header != *b"PK" {
        return Err(format!("{} is not a valid jar file", download.filename));
    }
    Ok(())
}

pub fn installed_pack(mods_folder: &Path) -> Option<InstalledPack> {
    fs::read(mods_folder.join(INSTALL_STATE))
        .ok()
        .and_then(|data| serde_json::from_slice(&data).ok())
}

fn version_parts(value: &str) -> Vec<u32> {
    value
        .split('.')
        .map(|part| part.parse().unwrap_or(0))
        .collect()
}

fn loader_order(loader: &str) -> (u8, &str) {
    let priority = match loader {
        "fabric" => 0,
        "neoforge" => 1,
        "forge" => 2,
        _ => 3,
    };
    (priority, loader)
}

fn branch_order(branch: &str) -> (u8, &str) {
    let priority = match branch {
        "nrc-standalone" => 0,
        "norisk-stable" => 1,
        "norisk-prod" => 2,
        "nrc-mini" => 3,
        "nrc-nightly" => 4,
        _ => 5,
    };
    (priority, branch)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_maven_url() {
        assert_eq!(
            maven_url(
                "https://repo/",
                "gg.norisk",
                "nrc",
                "1.0+fabric",
                "nrc-1.0.jar"
            ),
            "https://repo/gg/norisk/nrc/1.0+fabric/nrc-1.0.jar"
        );
    }
}

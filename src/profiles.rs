use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, PartialEq, Eq)]
pub struct GameProfile {
    pub name: String,
    pub launcher: String,
    pub mods_folder: PathBuf,
}

impl GameProfile {
    pub fn label(&self) -> String {
        format!("{} · {}", self.launcher, self.name)
    }
}

pub fn discover() -> Vec<GameProfile> {
    let mut profiles = Vec::new();

    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        add_official(
            &mut profiles,
            home.join("Library/Application Support/minecraft"),
        );
        add_prism_instances(
            &mut profiles,
            "Prism Launcher",
            home.join("Library/Application Support/PrismLauncher/instances"),
            "minecraft",
        );
        add_prism_instances(
            &mut profiles,
            "MultiMC",
            home.join("Library/Application Support/MultiMC/instances"),
            ".minecraft",
        );
        add_simple_instances(
            &mut profiles,
            "Modrinth App",
            home.join("Library/Application Support/ModrinthApp/profiles"),
            None,
        );
        add_simple_instances(
            &mut profiles,
            "Modrinth App",
            home.join("Library/Application Support/com.modrinth.theseus/profiles"),
            None,
        );
        add_simple_instances(
            &mut profiles,
            "CurseForge",
            home.join("curseforge/minecraft/Instances"),
            Some("minecraftinstance.json"),
        );
        add_simple_instances(
            &mut profiles,
            "CurseForge",
            home.join("Documents/curseforge/minecraft/Instances"),
            Some("minecraftinstance.json"),
        );
        add_simple_instances(
            &mut profiles,
            "ATLauncher",
            home.join("Library/Application Support/ATLauncher/instances"),
            Some("instance.json"),
        );
    }

    #[cfg(target_os = "windows")]
    {
        if let Some(app_data) = std::env::var_os("APPDATA").map(PathBuf::from) {
            add_official(&mut profiles, app_data.join(".minecraft"));
            add_prism_instances(
                &mut profiles,
                "Prism Launcher",
                app_data.join("PrismLauncher/instances"),
                "minecraft",
            );
            add_prism_instances(
                &mut profiles,
                "MultiMC",
                app_data.join("MultiMC/instances"),
                ".minecraft",
            );
            add_simple_instances(
                &mut profiles,
                "Modrinth App",
                app_data.join("ModrinthApp/profiles"),
                None,
            );
            add_simple_instances(
                &mut profiles,
                "Modrinth App",
                app_data.join("com.modrinth.theseus/profiles"),
                None,
            );
            add_simple_instances(
                &mut profiles,
                "ATLauncher",
                app_data.join("ATLauncher/instances"),
                Some("instance.json"),
            );
        }
        if let Some(home) = std::env::var_os("USERPROFILE").map(PathBuf::from) {
            add_simple_instances(
                &mut profiles,
                "CurseForge",
                home.join("curseforge/minecraft/Instances"),
                Some("minecraftinstance.json"),
            );
        }
    }

    #[cfg(target_os = "linux")]
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        add_official(&mut profiles, home.join(".minecraft"));
        add_prism_instances(
            &mut profiles,
            "Prism Launcher",
            home.join(".local/share/PrismLauncher/instances"),
            "minecraft",
        );
        add_prism_instances(
            &mut profiles,
            "MultiMC",
            home.join(".local/share/multimc/instances"),
            ".minecraft",
        );
        add_simple_instances(
            &mut profiles,
            "Modrinth App",
            home.join(".local/share/ModrinthApp/profiles"),
            None,
        );
        add_simple_instances(
            &mut profiles,
            "Modrinth App",
            home.join(".local/share/com.modrinth.theseus/profiles"),
            None,
        );
        add_simple_instances(
            &mut profiles,
            "ATLauncher",
            home.join(".local/share/atlauncher/instances"),
            Some("instance.json"),
        );
    }

    profiles.sort_by(|a, b| a.launcher.cmp(&b.launcher).then(a.name.cmp(&b.name)));
    profiles.dedup_by(|a, b| a.mods_folder == b.mods_folder);
    profiles
}

fn add_official(profiles: &mut Vec<GameProfile>, game_folder: PathBuf) {
    if !game_folder.is_dir() {
        return;
    }
    profiles.push(GameProfile {
        name: "Default profile".into(),
        launcher: "Minecraft Launcher".into(),
        mods_folder: game_folder.join("mods"),
    });

    let Ok(data) = fs::read(game_folder.join("launcher_profiles.json")) else {
        return;
    };
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(&data) else {
        return;
    };
    let Some(entries) = value
        .get("profiles")
        .and_then(|profiles| profiles.as_object())
    else {
        return;
    };
    for entry in entries.values() {
        let Some(game_dir) = entry.get("gameDir").and_then(|value| value.as_str()) else {
            continue;
        };
        let name = entry
            .get("name")
            .and_then(|value| value.as_str())
            .filter(|name| !name.is_empty())
            .unwrap_or("Custom profile");
        profiles.push(GameProfile {
            name: name.into(),
            launcher: "Minecraft Launcher".into(),
            mods_folder: PathBuf::from(game_dir).join("mods"),
        });
    }
}

fn add_prism_instances(
    profiles: &mut Vec<GameProfile>,
    launcher: &str,
    instances: PathBuf,
    game_folder: &str,
) {
    for instance in child_directories(&instances) {
        if !instance.join("instance.cfg").is_file() && !instance.join("mmc-pack.json").is_file() {
            continue;
        }
        let name = instance_name(&instance);
        profiles.push(GameProfile {
            name,
            launcher: launcher.into(),
            mods_folder: instance.join(game_folder).join("mods"),
        });
    }
}

fn add_simple_instances(
    profiles: &mut Vec<GameProfile>,
    launcher: &str,
    instances: PathBuf,
    marker: Option<&str>,
) {
    for instance in child_directories(&instances) {
        if marker.is_some_and(|marker| !instance.join(marker).is_file()) {
            continue;
        }
        profiles.push(GameProfile {
            name: instance_name(&instance),
            launcher: launcher.into(),
            mods_folder: instance.join("mods"),
        });
    }
}

fn child_directories(parent: &Path) -> Vec<PathBuf> {
    fs::read_dir(parent)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect()
}

fn instance_name(instance: &Path) -> String {
    let configured_name = fs::read_to_string(instance.join("instance.cfg"))
        .ok()
        .and_then(|contents| {
            contents
                .lines()
                .find_map(|line| line.strip_prefix("name="))
                .map(str::to_owned)
        });
    configured_name.unwrap_or_else(|| {
        instance
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Unnamed profile")
            .to_string()
    })
}

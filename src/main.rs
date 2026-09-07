#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod packs;
mod profiles;

use std::sync::mpsc::{self, Receiver};
use std::thread;

use eframe::egui;
use packs::{InstallProgress, InstalledPack, PackConfig};
use profiles::GameProfile;

const MANIFEST_URL: &str = "https://api.norisk.gg/api/v1/launcher/modpacks-v3";

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([560.0, 500.0])
            .with_min_inner_size([500.0, 440.0])
            .with_icon(app_icon()),
        ..Default::default()
    };

    eframe::run_native(
        "NoRisk Client Extractor",
        options,
        Box::new(|cc| Ok(Box::new(ExtractorApp::new(cc)))),
    )
}

#[cfg(target_os = "macos")]
fn app_icon() -> egui::IconData {
    egui::IconData::default()
}

#[cfg(not(target_os = "macos"))]
fn app_icon() -> egui::IconData {
    eframe::icon_data::from_png_bytes(include_bytes!("../assets/app-icon.png")).unwrap_or_default()
}

enum ManifestMessage {
    Loaded(PackConfig),
    Failed(String),
}

struct ExtractorApp {
    manifest: Option<PackConfig>,
    manifest_rx: Receiver<ManifestMessage>,
    install_rx: Option<Receiver<InstallProgress>>,
    game_version: String,
    loader: String,
    branch: String,
    profiles: Vec<GameProfile>,
    selected_profile: usize,
    installed: Option<InstalledPack>,
    status: String,
    progress: f32,
    installing: bool,
}

impl ExtractorApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        configure_style(&cc.egui_ctx);

        let (tx, rx) = mpsc::channel();
        let ctx = cc.egui_ctx.clone();
        thread::spawn(move || {
            let result = PackConfig::fetch(MANIFEST_URL)
                .map(ManifestMessage::Loaded)
                .unwrap_or_else(ManifestMessage::Failed);
            let _ = tx.send(result);
            ctx.request_repaint();
        });

        let profiles = profiles::discover();
        let mut app = Self {
            manifest: None,
            manifest_rx: rx,
            install_rx: None,
            game_version: String::new(),
            loader: String::new(),
            branch: String::new(),
            profiles,
            selected_profile: 0,
            installed: None,
            status: "Loading branches from NoRisk…".into(),
            progress: 0.0,
            installing: false,
        };
        if app.profiles.is_empty() {
            app.add_custom_profile(default_mods_folder());
        }
        app
    }

    fn receive_messages(&mut self) {
        while let Ok(message) = self.manifest_rx.try_recv() {
            match message {
                ManifestMessage::Loaded(manifest) => {
                    self.manifest = Some(manifest);
                    self.refresh_installed(true);
                    self.status = "Ready".into();
                }
                ManifestMessage::Failed(error) => self.status = error,
            }
        }

        let Some(rx) = &self.install_rx else {
            return;
        };
        let messages: Vec<_> = rx.try_iter().collect();

        for message in messages {
            match message {
                InstallProgress::Progress {
                    completed,
                    total,
                    filename,
                } => {
                    self.progress = completed as f32 / total.max(1) as f32;
                    self.status = format!("Installing {filename} ({completed}/{total})");
                }
                InstallProgress::Finished(count) => {
                    self.progress = 1.0;
                    self.status = format!("Installed {count} mods successfully");
                    self.installing = false;
                    self.refresh_installed(false);
                }
                InstallProgress::Failed(error) => {
                    self.status = error;
                    self.installing = false;
                }
            }
        }
    }

    fn reconcile_selection(&mut self) {
        let Some(manifest) = &self.manifest else {
            return;
        };

        let versions = manifest.game_versions();
        if !versions.contains(&self.game_version) {
            self.game_version = versions.first().cloned().unwrap_or_default();
        }

        let loaders = manifest.loaders_for(&self.game_version);
        if !loaders.contains(&self.loader) {
            self.loader = loaders.first().cloned().unwrap_or_default();
        }

        let branches = manifest.branches_for(&self.game_version, &self.loader);
        if !branches.iter().any(|branch| branch.id == self.branch) {
            self.branch = branches
                .first()
                .map(|branch| branch.id.clone())
                .unwrap_or_default();
        }
    }

    fn start_install(&mut self, ctx: &egui::Context) {
        let Some(manifest) = self.manifest.clone() else {
            return;
        };

        let folder = self.mods_folder().to_path_buf();
        let version = self.game_version.clone();
        let loader = self.loader.clone();
        let branch = self.branch.clone();
        let (tx, rx) = mpsc::channel();
        let repaint = ctx.clone();

        self.installing = true;
        self.progress = 0.0;
        self.status = "Preparing installation…".into();
        self.install_rx = Some(rx);

        thread::spawn(move || {
            let result = manifest.install(&branch, &version, &loader, &folder, |progress| {
                let _ = tx.send(progress);
                repaint.request_repaint();
            });
            match result {
                Ok(count) => {
                    let _ = tx.send(InstallProgress::Finished(count));
                }
                Err(error) => {
                    let _ = tx.send(InstallProgress::Failed(error));
                }
            }
            repaint.request_repaint();
        });
    }

    fn mods_folder(&self) -> &std::path::Path {
        &self.profiles[self.selected_profile].mods_folder
    }

    fn refresh_installed(&mut self, select_detected: bool) {
        self.installed = self
            .manifest
            .as_ref()
            .and_then(|manifest| manifest.detect_install(self.mods_folder()));

        if select_detected {
            if let Some(installed) = &self.installed {
                if !installed.branch.is_empty() {
                    self.game_version.clone_from(&installed.game_version);
                    self.loader.clone_from(&installed.loader);
                    self.branch.clone_from(&installed.branch);
                }
            }
        }
        self.reconcile_selection();
    }

    fn add_custom_profile(&mut self, mods_folder: std::path::PathBuf) {
        if let Some(index) = self
            .profiles
            .iter()
            .position(|profile| profile.mods_folder == mods_folder)
        {
            self.selected_profile = index;
            return;
        }
        let name = mods_folder
            .parent()
            .and_then(|folder| folder.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("Custom folder")
            .to_string();
        self.profiles.push(GameProfile {
            name,
            launcher: "Custom".into(),
            mods_folder,
        });
        self.selected_profile = self.profiles.len() - 1;
    }

    fn action_label(&self) -> &'static str {
        match &self.installed {
            None => "Install NoRisk pack",
            Some(installed)
                if installed.branch == self.branch
                    && installed.game_version == self.game_version
                    && installed.loader == self.loader =>
            {
                "Update NoRisk pack"
            }
            Some(_) => "Switch NoRisk pack",
        }
    }
}

impl eframe::App for ExtractorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.receive_messages();
        ctx.request_repaint_after(std::time::Duration::from_millis(150));

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.add_space(16.0);
            ui.heading("NoRisk Client Extractor");
            ui.add_space(20.0);

            ui.add_enabled_ui(!self.installing && self.manifest.is_some(), |ui| {
                egui::Grid::new("choices")
                    .num_columns(2)
                    .spacing([18.0, 14.0])
                    .show(ui, |ui| {
                        ui.label("Launcher profile");
                        let previous_folder = self.mods_folder().to_path_buf();
                        egui::ComboBox::from_id_salt("profile")
                            .selected_text(self.profiles[self.selected_profile].label())
                            .width(280.0)
                            .show_ui(ui, |ui| {
                                for (index, profile) in self.profiles.iter().enumerate() {
                                    ui.selectable_value(
                                        &mut self.selected_profile,
                                        index,
                                        profile.label(),
                                    );
                                }
                            });
                        ui.end_row();

                        ui.label("");
                        ui.horizontal(|ui| {
                            if ui.small_button("Rescan profiles").clicked() {
                                let selected_folder = self.mods_folder().to_path_buf();
                                self.profiles = profiles::discover();
                                if self.profiles.is_empty() {
                                    self.add_custom_profile(selected_folder.clone());
                                }
                                self.selected_profile = self
                                    .profiles
                                    .iter()
                                    .position(|profile| profile.mods_folder == selected_folder)
                                    .unwrap_or(0);
                            }
                            if ui.small_button("Choose custom folder...").clicked() {
                                if let Some(folder) = rfd::FileDialog::new()
                                    .set_directory(self.mods_folder())
                                    .pick_folder()
                                {
                                    self.add_custom_profile(folder);
                                }
                            }
                        });
                        ui.end_row();

                        if previous_folder != self.mods_folder() {
                            self.refresh_installed(true);
                        }

                        ui.label("Game version");
                        let versions = self
                            .manifest
                            .as_ref()
                            .map(PackConfig::game_versions)
                            .unwrap_or_default();
                        egui::ComboBox::from_id_salt("game_version")
                            .selected_text(selection_text(&self.game_version))
                            .width(250.0)
                            .show_ui(ui, |ui| {
                                for version in versions {
                                    ui.selectable_value(
                                        &mut self.game_version,
                                        version.clone(),
                                        version,
                                    );
                                }
                            });
                        ui.end_row();

                        ui.label("Loader");
                        let loaders = self
                            .manifest
                            .as_ref()
                            .map(|manifest| manifest.loaders_for(&self.game_version))
                            .unwrap_or_default();
                        egui::ComboBox::from_id_salt("loader")
                            .selected_text(selection_text(&self.loader))
                            .width(250.0)
                            .show_ui(ui, |ui| {
                                for loader in loaders {
                                    ui.selectable_value(
                                        &mut self.loader,
                                        loader.clone(),
                                        title_case(&loader),
                                    );
                                }
                            });
                        ui.end_row();

                        ui.label("NoRisk branch");
                        let branches = self
                            .manifest
                            .as_ref()
                            .map(|manifest| manifest.branches_for(&self.game_version, &self.loader))
                            .unwrap_or_default();
                        egui::ComboBox::from_id_salt("branch")
                            .selected_text(
                                branches
                                    .iter()
                                    .find(|item| item.id == self.branch)
                                    .map(|item| item.display_name.as_str())
                                    .unwrap_or("Select…"),
                            )
                            .width(250.0)
                            .show_ui(ui, |ui| {
                                for item in branches {
                                    ui.selectable_value(
                                        &mut self.branch,
                                        item.id.clone(),
                                        &item.display_name,
                                    );
                                }
                            });
                        ui.end_row();

                        ui.label("Mods folder");
                        ui.label(self.mods_folder().display().to_string());
                        ui.end_row();
                    });

                self.reconcile_selection();
            });

            ui.add_space(22.0);
            if let Some(installed) = &self.installed {
                if installed.branch.is_empty() {
                    ui.label("Detected a NoRisk pack previously used with this tool.");
                } else {
                    ui.label(format!(
                        "Installed: {} · {} · {}",
                        installed.branch,
                        installed.game_version,
                        title_case(&installed.loader)
                    ));
                }
                ui.add_space(8.0);
            }
            if self.installing {
                ui.add(
                    egui::ProgressBar::new(self.progress)
                        .animate(true)
                        .show_percentage(),
                );
            } else {
                let can_install = self.manifest.is_some()
                    && !self.game_version.is_empty()
                    && !self.loader.is_empty()
                    && !self.branch.is_empty();
                if ui
                    .add_enabled(
                        can_install,
                        egui::Button::new(self.action_label()).min_size([180.0, 34.0].into()),
                    )
                    .clicked()
                {
                    self.start_install(ctx);
                }
            }

            ui.add_space(10.0);
            ui.label(&self.status);
        });
    }
}

fn configure_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.visuals = egui::Visuals::dark();
    ctx.set_style(style);
}

fn default_mods_folder() -> std::path::PathBuf {
    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME") {
        return std::path::PathBuf::from(home).join("Library/Application Support/minecraft/mods");
    }

    #[cfg(target_os = "windows")]
    if let Some(app_data) = std::env::var_os("APPDATA") {
        return std::path::PathBuf::from(app_data).join(".minecraft/mods");
    }

    std::env::current_dir().unwrap_or_default().join("mods")
}

fn selection_text(value: &str) -> &str {
    if value.is_empty() { "Select…" } else { value }
}

fn title_case(value: &str) -> String {
    let mut chars = value.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

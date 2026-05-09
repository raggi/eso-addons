use std::path::PathBuf;

use eframe::egui::{self, Button, Layout, RichText};
use eso_addons_core::{
    addon_path::{self, AddonPathCandidate, AddonPathStatus},
    config::detect_addon_dir,
    service::AddonService,
};
use lazy_async_promise::ImmediateValuePromise;
use rfd::AsyncFileDialog;

use crate::views::View;

use super::ui_helpers::{AddonResponse, PromisedValue};

pub struct Onboard {
    addon_dir_dialog: PromisedValue<Option<String>>,
    addon_dir_set: bool,
    setup_done: bool,
    candidates: Vec<AddonPathCandidate>,
}

impl Default for Onboard {
    fn default() -> Self {
        Self {
            addon_dir_dialog: PromisedValue::default(),
            addon_dir_set: false,
            setup_done: false,
            candidates: addon_path::detect_candidates(),
        }
    }
}
impl Onboard {
    fn poll(&mut self, service: &mut AddonService) {
        self.addon_dir_dialog.poll();
        if self.addon_dir_dialog.is_ready() {
            self.addon_dir_dialog.handle();
            let value = self.addon_dir_dialog.value.as_ref().unwrap();
            if let Some(path) = value {
                self.apply_path(service, PathBuf::from(path));
            } else {
                self.addon_dir_set = false;
            }
        }
    }
    fn apply_path(&mut self, service: &mut AddonService, path: PathBuf) {
        service.config.addon_dir = path;
        service.save_config();
        self.addon_dir_set = true;
    }
    pub fn is_setup_done(&self) -> bool {
        self.addon_dir_set
    }
}
impl View for Onboard {
    fn ui(
        &mut self,
        _ctx: &eframe::egui::Context,
        ui: &mut eframe::egui::Ui,
        service: &mut AddonService,
    ) -> AddonResponse {
        let response = AddonResponse::default();

        self.poll(service);

        ui.add_space(5.0);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Welcome to the Unofficial ESO AddOn Manager!")
                    .heading()
                    .strong(),
            );
            ui.with_layout(Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add_enabled(
                        self.is_setup_done(),
                        egui::Button::new(RichText::new("Done!").heading()),
                    )
                    .clicked()
                {
                    self.setup_done = true;
                    service.config.onboard = false;
                    service.save_config();
                }
            });
        });
        ui.add_space(5.0);

        ui.heading("Let's start by finding the right folder to save your AddOns:");
        ui.add_space(5.0);

        let mut chosen: Option<PathBuf> = None;
        if !self.candidates.is_empty() {
            ui.label(RichText::new("Detected locations:").strong());
            ui.add_space(3.0);
            for candidate in &self.candidates {
                ui.horizontal(|ui| {
                    let icon = match candidate.status {
                        AddonPathStatus::AddOnsExists => "✔",
                        AddonPathStatus::LiveExists => "▶",
                        AddonPathStatus::GameSavedataExists => "○",
                        AddonPathStatus::PrefixExists => "•",
                    };
                    let label = format!("{} {}", icon, candidate.source.label());
                    if ui.button(RichText::new(label).heading()).clicked() {
                        chosen = Some(candidate.path.clone());
                    }
                    ui.vertical(|ui| {
                        ui.label(candidate.path.to_string_lossy());
                        ui.label(
                            RichText::new(candidate.status.label())
                                .small()
                                .weak(),
                        );
                    });
                });
                ui.add_space(2.0);
            }
            ui.separator();
            ui.add_space(3.0);
        }
        if let Some(path) = chosen {
            self.apply_path(service, path);
        }

        if self.addon_dir_dialog.is_polling() {
            ui.add_enabled(
                false,
                Button::new(RichText::new("Choose another folder...").heading()),
            );
        } else if ui
            .button(RichText::new("Choose another folder...").heading())
            .clicked()
        {
            let promise = ImmediateValuePromise::new(async move {
                let dialog = AsyncFileDialog::new()
                    .set_directory(detect_addon_dir())
                    .pick_folder()
                    .await;
                if let Some(path) = dialog {
                    return Ok(Some(path.path().to_string_lossy().to_string()));
                }
                Ok(None::<String>)
            });
            self.addon_dir_dialog.set(promise);
        }
        ui.add_space(5.0);
        ui.label(
            service
                .config
                .addon_dir
                .clone()
                .into_os_string()
                .to_str()
                .unwrap(),
        );

        response
    }
}

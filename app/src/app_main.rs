//! `sfcwc-app` — minimal eframe/egui shell.
//!
//! M1.1 ships a read-only viewer: open an M1 project file, render
//! the Sample Pool, surface validation errors. No editing, no
//! import. Substance lands at M1.2+.
//!
//! Native file pickers (`rfd` etc.) are not in the M1.1 authorized
//! dep set, so File → Open / Save As / New uses a hand-rolled
//! single-line text-input modal. M1.2+ may upgrade once an
//! authorized native-picker crate lands.
//!
//! Optional CLI: `sfcwc-app <path>` opens that project on launch.

use std::path::{Path, PathBuf};

use eframe::egui;
use sfc_atomizer_core::project::{ProjectV1, SampleSlot, ValidationError};

const WINDOW_DEFAULT_WIDTH: f32 = 1024.0;
const WINDOW_DEFAULT_HEIGHT: f32 = 640.0;

fn main() -> Result<(), eframe::Error> {
    let initial_project_arg = std::env::args().nth(1);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([WINDOW_DEFAULT_WIDTH, WINDOW_DEFAULT_HEIGHT])
            .with_title("SFC Wave Compiler"),
        ..Default::default()
    };
    eframe::run_native(
        "SFC Wave Compiler",
        options,
        Box::new(move |_cc| {
            let mut app = SfcwcApp::default();
            if let Some(path) = initial_project_arg {
                app.try_open(&PathBuf::from(path));
            }
            Ok(Box::new(app))
        }),
    )
}

#[derive(Default)]
struct SfcwcApp {
    project: Option<ProjectV1>,
    project_path: Option<PathBuf>,
    validation_errors: Vec<ValidationError>,
    selected_sample_id: Option<String>,

    // Modal state.
    open_modal: ModalState,
    save_as_modal: ModalState,
    new_modal: NewModalState,
    show_errors_modal: bool,

    // One-shot status message (e.g. "loaded /tmp/x.json").
    status_message: Option<String>,
}

#[derive(Default)]
struct ModalState {
    visible: bool,
    path_input: String,
}

#[derive(Default)]
struct NewModalState {
    visible: bool,
    path_input: String,
    name_input: String,
}

impl SfcwcApp {
    fn try_open(&mut self, path: &Path) {
        match ProjectV1::load_from_path(path) {
            Ok(p) => {
                let errors = p.validate().err().unwrap_or_default();
                self.project = Some(p);
                self.project_path = Some(path.to_path_buf());
                self.validation_errors = errors;
                self.selected_sample_id = None;
                self.status_message = Some(format!("loaded {}", path.display()));
            }
            Err(e) => {
                self.project = None;
                self.project_path = None;
                self.validation_errors = Vec::new();
                self.status_message = Some(format!("load failed: {e}"));
            }
        }
    }

    fn try_save(&mut self, path: &Path) {
        let Some(project) = self.project.as_ref() else {
            self.status_message = Some("no project loaded".to_string());
            return;
        };
        match project.save_to_path(path) {
            Ok(()) => {
                self.project_path = Some(path.to_path_buf());
                // Re-validate after save (file content may equal what
                // was already in memory, but the spec also requires
                // validation on Save As).
                self.validation_errors = project.validate().err().unwrap_or_default();
                self.status_message = Some(format!("saved {}", path.display()));
            }
            Err(e) => {
                self.status_message = Some(format!("save failed: {e}"));
            }
        }
    }

    fn try_new(&mut self, path: &Path, name: &str) {
        let template = ProjectV1::new_template(name);
        match template.save_to_path(path) {
            Ok(()) => {
                self.project_path = Some(path.to_path_buf());
                self.validation_errors = template.validate().err().unwrap_or_default();
                self.project = Some(template);
                self.selected_sample_id = None;
                self.status_message = Some(format!("created {}", path.display()));
            }
            Err(e) => {
                self.status_message = Some(format!("new project failed: {e}"));
            }
        }
    }
}

impl eframe::App for SfcwcApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Window title reflects loaded project.
        let title = match &self.project {
            Some(p) => format!("SFC Wave Compiler — {}", p.project.name),
            None => "SFC Wave Compiler".to_string(),
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));

        self.draw_top_menu(ctx);
        self.draw_left_panel(ctx);
        self.draw_bottom_status(ctx);
        self.draw_center(ctx);

        self.draw_open_modal(ctx);
        self.draw_save_as_modal(ctx);
        self.draw_new_modal(ctx);
        self.draw_errors_modal(ctx);
    }
}

// ============================================================================
// UI rendering
// ============================================================================

impl SfcwcApp {
    fn draw_top_menu(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top_menu").show(ctx, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.button("New Project…").clicked() {
                        self.new_modal.path_input = "untitled.sfcproj.json".to_string();
                        self.new_modal.name_input = "untitled".to_string();
                        self.new_modal.visible = true;
                        ui.close();
                    }
                    if ui.button("Open Project…").clicked() {
                        self.open_modal.path_input = self
                            .project_path
                            .as_ref()
                            .map(|p| p.display().to_string())
                            .unwrap_or_default();
                        self.open_modal.visible = true;
                        ui.close();
                    }
                    let save_enabled = self.project.is_some() && self.project_path.is_some();
                    if ui
                        .add_enabled(save_enabled, egui::Button::new("Save Project"))
                        .clicked()
                    {
                        if let Some(path) = self.project_path.clone() {
                            self.try_save(&path);
                        }
                        ui.close();
                    }
                    if ui
                        .add_enabled(
                            self.project.is_some(),
                            egui::Button::new("Save Project As…"),
                        )
                        .clicked()
                    {
                        self.save_as_modal.path_input = self
                            .project_path
                            .as_ref()
                            .map(|p| p.display().to_string())
                            .unwrap_or_else(|| "untitled.sfcproj.json".to_string());
                        self.save_as_modal.visible = true;
                        ui.close();
                    }
                    ui.separator();
                    if ui.button("Quit").clicked() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        ui.close();
                    }
                });
            });
        });
    }

    fn draw_left_panel(&mut self, ctx: &egui::Context) {
        egui::SidePanel::left("sample_pool")
            .resizable(true)
            .default_width(280.0)
            .show(ctx, |ui| {
                ui.heading("Sample Pool");
                ui.separator();
                let Some(project) = self.project.as_ref() else {
                    ui.vertical_centered(|ui| {
                        ui.add_space(12.0);
                        ui.label("(no project loaded)");
                    });
                    return;
                };
                if project.sample_pool.is_empty() {
                    ui.vertical_centered(|ui| {
                        ui.add_space(12.0);
                        ui.label("No samples imported yet.");
                        ui.label("(Import lands at M1.2.)");
                    });
                    return;
                }
                egui::ScrollArea::vertical().show(ui, |ui| {
                    let selected = self.selected_sample_id.clone();
                    for s in &project.sample_pool {
                        let is_selected = selected.as_deref() == Some(s.id.as_str());
                        let label = format!(
                            "{} ({})\n  {} — {} frames",
                            s.id,
                            s.name,
                            format_format(&s.source.format),
                            s.source.frames
                        );
                        if ui.selectable_label(is_selected, label).clicked() {
                            self.selected_sample_id = Some(s.id.clone());
                        }
                    }
                });
            });
    }

    fn draw_bottom_status(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("status_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                match (&self.project_path, &self.project) {
                    (Some(path), Some(_)) => {
                        ui.label(format!("Loaded: {}", path.display()));
                        ui.separator();
                        if self.validation_errors.is_empty() {
                            ui.label("Valid: ✓");
                        } else {
                            ui.label(format!(
                                "Valid: ✗ ({} errors)",
                                self.validation_errors.len()
                            ));
                            if ui.button("Show errors").clicked() {
                                self.show_errors_modal = true;
                            }
                        }
                    }
                    _ => {
                        ui.label("No project loaded.");
                    }
                }
                if let Some(msg) = self.status_message.as_deref() {
                    ui.separator();
                    ui.label(msg);
                }
            });
        });
    }

    fn draw_center(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            let Some(project) = self.project.as_ref() else {
                ui.vertical_centered(|ui| {
                    ui.add_space(40.0);
                    ui.label("Open a project from the File menu.");
                    ui.add_space(8.0);
                    ui.weak("File → Open Project…");
                });
                return;
            };
            match self.selected_sample_id.as_deref() {
                Some(id) => match project.sample_pool.iter().find(|s| s.id == id) {
                    Some(s) => draw_sample_detail(ui, s),
                    None => {
                        ui.weak(format!("(selected sample {id} not in pool)"));
                    }
                },
                None => draw_project_detail(ui, project),
            }
        });
    }

    fn draw_open_modal(&mut self, ctx: &egui::Context) {
        if !self.open_modal.visible {
            return;
        }
        let mut close = false;
        let mut do_open = false;
        egui::Window::new("Open Project")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label("Project file path:");
                ui.text_edit_singleline(&mut self.open_modal.path_input);
                ui.horizontal(|ui| {
                    if ui.button("Open").clicked() {
                        do_open = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            });
        if do_open {
            let p = PathBuf::from(self.open_modal.path_input.trim());
            self.try_open(&p);
            self.open_modal.visible = false;
        } else if close {
            self.open_modal.visible = false;
        }
    }

    fn draw_save_as_modal(&mut self, ctx: &egui::Context) {
        if !self.save_as_modal.visible {
            return;
        }
        let mut close = false;
        let mut do_save = false;
        egui::Window::new("Save Project As")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label("Save to:");
                ui.text_edit_singleline(&mut self.save_as_modal.path_input);
                ui.horizontal(|ui| {
                    if ui.button("Save").clicked() {
                        do_save = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            });
        if do_save {
            let p = PathBuf::from(self.save_as_modal.path_input.trim());
            self.try_save(&p);
            self.save_as_modal.visible = false;
        } else if close {
            self.save_as_modal.visible = false;
        }
    }

    fn draw_new_modal(&mut self, ctx: &egui::Context) {
        if !self.new_modal.visible {
            return;
        }
        let mut close = false;
        let mut do_new = false;
        egui::Window::new("New Project")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label("Project name:");
                ui.text_edit_singleline(&mut self.new_modal.name_input);
                ui.label("Save to:");
                ui.text_edit_singleline(&mut self.new_modal.path_input);
                ui.horizontal(|ui| {
                    if ui.button("Create").clicked() {
                        do_new = true;
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            });
        if do_new {
            let p = PathBuf::from(self.new_modal.path_input.trim());
            let n = self.new_modal.name_input.trim().to_string();
            self.try_new(&p, &n);
            self.new_modal.visible = false;
        } else if close {
            self.new_modal.visible = false;
        }
    }

    fn draw_errors_modal(&mut self, ctx: &egui::Context) {
        if !self.show_errors_modal {
            return;
        }
        let mut open = self.show_errors_modal;
        egui::Window::new("Validation errors")
            .collapsible(false)
            .resizable(true)
            .open(&mut open)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                if self.validation_errors.is_empty() {
                    ui.label("(no errors)");
                    return;
                }
                egui::ScrollArea::vertical()
                    .max_height(400.0)
                    .show(ui, |ui| {
                        for e in &self.validation_errors {
                            ui.label(format!("{} : {}", e.path, e.kind));
                        }
                    });
            });
        self.show_errors_modal = open;
    }
}

fn draw_project_detail(ui: &mut egui::Ui, project: &ProjectV1) {
    ui.heading("Project");
    ui.separator();
    egui::Grid::new("project_grid")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("name");
            ui.monospace(&project.project.name);
            ui.end_row();
            ui.label("tick_rate_hz");
            ui.monospace(project.project.tick_rate_hz.to_string());
            ui.end_row();
            ui.label("driver.profile");
            ui.monospace(&project.driver.profile);
            ui.end_row();
            ui.label("driver.bytecode_version");
            ui.monospace(project.driver.bytecode_version.to_string());
            ui.end_row();
            ui.label("master_echo.enabled");
            ui.monospace(project.master_echo.enabled.to_string());
            ui.end_row();
            ui.label("master_echo.edl");
            ui.monospace(project.master_echo.edl.to_string());
            ui.end_row();
            ui.label("sample_pool.len");
            ui.monospace(project.sample_pool.len().to_string());
            ui.end_row();
            ui.label("m1.active_sample_id");
            ui.monospace(if project.m1.active_sample_id.is_empty() {
                "(none)"
            } else {
                project.m1.active_sample_id.as_str()
            });
            ui.end_row();
        });
}

fn draw_sample_detail(ui: &mut egui::Ui, s: &SampleSlot) {
    ui.heading(format!("Sample — {}", s.id));
    ui.separator();
    egui::ScrollArea::vertical().show(ui, |ui| {
        egui::Grid::new("sample_grid")
            .num_columns(2)
            .show(ui, |ui| {
                ui.label("id");
                ui.monospace(&s.id);
                ui.end_row();
                ui.label("name");
                ui.monospace(&s.name);
                ui.end_row();
                ui.label("source.path");
                ui.monospace(&s.source.path);
                ui.end_row();
                ui.label("source.format");
                ui.monospace(format_format(&s.source.format));
                ui.end_row();
                ui.label("source.sample_rate_hz");
                ui.monospace(s.source.sample_rate_hz.to_string());
                ui.end_row();
                ui.label("source.channels");
                ui.monospace(s.source.channels.to_string());
                ui.end_row();
                ui.label("source.frames");
                ui.monospace(s.source.frames.to_string());
                ui.end_row();
                ui.label("source.sha256");
                ui.monospace(&s.source.sha256);
                ui.end_row();
                ui.label("root_midi_note");
                ui.monospace(s.root_midi_note.to_string());
                ui.end_row();
                ui.label("loop.enabled");
                ui.monospace(s.looped.enabled.to_string());
                ui.end_row();
                if s.looped.enabled {
                    ui.label("loop.start_sample");
                    ui.monospace(format!("{:?}", s.looped.start_sample));
                    ui.end_row();
                    ui.label("loop.end_sample");
                    ui.monospace(format!("{:?}", s.looped.end_sample));
                    ui.end_row();
                }
                ui.label("playback.volume");
                ui.monospace(format!("{:.3}", s.playback.volume));
                ui.end_row();
                ui.label("playback.pan");
                ui.monospace(format!("{:.3}", s.playback.pan));
                ui.end_row();
                ui.label("playback.echo");
                ui.monospace(s.playback.echo.to_string());
                ui.end_row();
                ui.label("envelope.type");
                use sfc_atomizer_core::project::Envelope;
                match &s.playback.envelope {
                    Envelope::Adsr {
                        attack,
                        decay,
                        sustain_level,
                        sustain_rate,
                    } => {
                        ui.monospace("adsr");
                        ui.end_row();
                        ui.label("envelope.attack");
                        ui.monospace(attack.to_string());
                        ui.end_row();
                        ui.label("envelope.decay");
                        ui.monospace(decay.to_string());
                        ui.end_row();
                        ui.label("envelope.sustain_level");
                        ui.monospace(sustain_level.to_string());
                        ui.end_row();
                        ui.label("envelope.sustain_rate");
                        ui.monospace(sustain_rate.to_string());
                        ui.end_row();
                    }
                    Envelope::GainRaw { gain_byte } => {
                        ui.monospace("gain_raw");
                        ui.end_row();
                        ui.label("envelope.gain_byte");
                        ui.monospace(gain_byte.to_string());
                        ui.end_row();
                    }
                }
            });
    });
}

fn format_format(f: &sfc_atomizer_core::project::SampleFormat) -> &'static str {
    use sfc_atomizer_core::project::SampleFormat;
    match f {
        SampleFormat::Wav => "wav",
        SampleFormat::Aiff => "aiff",
        SampleFormat::Brr => "brr",
    }
}

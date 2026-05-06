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
use sfc_atomizer_core::audio::decode_to_mono_pcm;
use sfc_atomizer_core::audition::export_decoded_brr_wav;
use sfc_atomizer_core::brr_encoder::{encode as brr_encode, encode_looped, EncodeOptions};
use sfc_atomizer_core::driver_build::{build as driver_build, DriverBuildInput};
use sfc_atomizer_core::import::{import_audio, ImportOptions};
use sfc_atomizer_core::loop_finder::{find_loop_candidates, LoopCandidate, LoopFinderOptions};
use sfc_atomizer_core::module_writer::{
    parse_module_blocks, parse_module_header, project_blocks_to_aram,
};
use sfc_atomizer_core::packer::{pack as packer_pack, EncodedSample, PackInput};
use sfc_atomizer_core::project::{ProjectV1, SampleSlot, ValidationError};
use sfc_atomizer_core::report::{AramKind, AramMapReport};
use sfc_atomizer_core::sfc_export::{export_sfc, SfcExportInput, MODULE_A_FILE_OFFSET};
use sfc_atomizer_core::spc::build_m1_image;
use sfc_atomizer_core::tools::resolve_snes_spc_oracle;

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
    loop_candidates_modal: LoopCandidatesModalState,

    /// Cached ARAM map for the meter view. Invalidated on project
    /// load, sample edits, and manual refresh; encoder + packer pass
    /// is too expensive to run every frame.
    aram_meter: Option<AramMapReport>,
    /// Stashed error message when the last recompute failed (e.g. a
    /// sample's source file wasn't where the project said it was).
    /// Surfaced under the meter so the user knows why it's stale.
    aram_meter_error: Option<String>,

    /// Path to the most recently compiled `.spc`. Enables the
    /// Verify Audible button.
    last_compiled_spc: Option<PathBuf>,
    /// Free-form summary line for the last compile result.
    last_compile_summary: Option<String>,
    /// Free-form summary line for the last audible-verify result;
    /// includes max_abs / rms / status.
    last_audible_summary: Option<String>,
    /// Severity of the last audible-verify run for color rendering.
    last_audible_ok: Option<bool>,

    /// Path to the most recently compiled `.sfc` (M1.6).
    last_compiled_sfc: Option<PathBuf>,
    last_sfc_compile_summary: Option<String>,
    last_sfc_verify_summary: Option<String>,
    last_sfc_verify_ok: Option<bool>,

    // One-shot status message (e.g. "loaded /tmp/x.json").
    status_message: Option<String>,
}

#[derive(Default)]
struct LoopCandidatesModalState {
    visible: bool,
    /// Sample id the candidates were computed for; clicking Apply on a
    /// candidate writes back to this id, so a sample switch in between
    /// closes the modal rather than corrupting the wrong slot.
    target_sample_id: Option<String>,
    candidates: Vec<LoopCandidate>,
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
                self.recompute_aram_meter();
            }
            Err(e) => {
                self.project = None;
                self.project_path = None;
                self.validation_errors = Vec::new();
                self.status_message = Some(format!("load failed: {e}"));
                self.aram_meter = None;
                self.aram_meter_error = None;
            }
        }
    }

    /// Synchronously decode + encode every sample, run the packer, and
    /// stash the resulting [`AramMapReport`] for the meter view. On
    /// any failure (missing audio, decode error, pack overflow), leave
    /// the previous meter untouched and stash an error string.
    fn recompute_aram_meter(&mut self) {
        let Some(project) = self.project.as_ref() else {
            self.aram_meter = None;
            self.aram_meter_error = None;
            return;
        };
        if !self.validation_errors.is_empty() {
            self.aram_meter = None;
            self.aram_meter_error = Some("project has validation errors".to_string());
            return;
        }
        let project_dir = self
            .project_path
            .as_ref()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."));
        let mut encoded: Vec<EncodedSample> = Vec::with_capacity(project.sample_pool.len());
        for slot in &project.sample_pool {
            let raw = Path::new(&slot.source.path);
            let audio_path = if raw.is_absolute() {
                raw.to_path_buf()
            } else {
                project_dir.join(raw)
            };
            let pcm = match decode_to_mono_pcm(&audio_path) {
                Ok(p) => p,
                Err(e) => {
                    self.aram_meter_error = Some(format!("decode {}: {e}", slot.source.path));
                    return;
                }
            };
            let opts = EncodeOptions::default();
            let (bytes, loop_entry_block) = if slot.looped.enabled {
                match slot.looped.start_sample {
                    Some(start) => match encode_looped(&pcm, start, &opts) {
                        Ok(r) => (r.bytes, Some(start / 16)),
                        Err(e) => {
                            self.aram_meter_error = Some(format!("encode_looped {}: {e}", slot.id));
                            return;
                        }
                    },
                    None => (brr_encode(&pcm, &opts).bytes, None),
                }
            } else {
                (brr_encode(&pcm, &opts).bytes, None)
            };
            encoded.push(EncodedSample {
                sample_id: slot.id.clone(),
                bytes,
                loop_entry_block,
            });
        }
        match packer_pack(PackInput {
            project: project.clone(),
            encoded_samples: encoded,
            driver_code: Vec::new(),
        }) {
            Ok(out) => {
                self.aram_meter = Some(out.map_report);
                self.aram_meter_error = None;
            }
            Err(e) => {
                self.aram_meter_error = Some(format!("pack: {e}"));
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
                self.recompute_aram_meter();
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
        self.draw_loop_candidates_modal(ctx);
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
                    let import_enabled = self.project.is_some() && self.project_path.is_some();
                    if ui
                        .add_enabled(import_enabled, egui::Button::new("Import Audio…"))
                        .clicked()
                    {
                        self.do_import_via_dialog();
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

    fn do_import_via_dialog(&mut self) {
        let Some(project_path) = self.project_path.clone() else {
            self.status_message = Some("import: no project loaded".to_string());
            return;
        };
        let picked = rfd::FileDialog::new()
            .add_filter(
                "Audio (wav, aif, aiff, aifc, brr)",
                &["wav", "aif", "aiff", "aifc", "brr"],
            )
            .pick_file();
        let Some(audio_path) = picked else {
            self.status_message = Some("import: cancelled".to_string());
            return;
        };
        match import_audio(&project_path, &audio_path, ImportOptions::copy_default()) {
            Ok(r) => {
                self.status_message = Some(format!(
                    "import: added {} ({} frames) from {}",
                    r.sample_id,
                    r.metadata.frames,
                    audio_path.display()
                ));
                // Refresh in-memory state from disk so the Sample Pool
                // panel renders the new entry.
                self.try_open(&project_path);
            }
            Err(e) => {
                self.status_message = Some(format!("import failed: {e}"));
            }
        }
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
        let selected = self.selected_sample_id.clone();
        let mut response = SampleDetailResponse::default();
        let mut project_resp = ProjectDetailResponse::default();
        let aram_meter = self.aram_meter.clone();
        let aram_meter_error = self.aram_meter_error.clone();
        let last_compile_summary = self.last_compile_summary.clone();
        let last_audible_summary = self.last_audible_summary.clone();
        let last_audible_ok = self.last_audible_ok;
        let has_compiled_spc = self.last_compiled_spc.is_some();
        let last_sfc_compile_summary = self.last_sfc_compile_summary.clone();
        let last_sfc_verify_summary = self.last_sfc_verify_summary.clone();
        let last_sfc_verify_ok = self.last_sfc_verify_ok;
        let has_compiled_sfc = self.last_compiled_sfc.is_some();
        egui::CentralPanel::default().show(ctx, |ui| {
            let Some(project) = self.project.as_mut() else {
                ui.vertical_centered(|ui| {
                    ui.add_space(40.0);
                    ui.label("Open a project from the File menu.");
                    ui.add_space(8.0);
                    ui.weak("File → Open Project…");
                });
                return;
            };
            match selected.as_deref() {
                Some(id) => match project.sample_pool.iter_mut().find(|s| s.id == id) {
                    Some(s) => {
                        response = draw_sample_detail(ui, s);
                    }
                    None => {
                        ui.weak(format!("(selected sample {id} not in pool)"));
                    }
                },
                None => {
                    project_resp = draw_project_detail(
                        ui,
                        ProjectDetailState {
                            project,
                            aram_meter: aram_meter.as_ref(),
                            aram_meter_error: aram_meter_error.as_deref(),
                            last_compile_summary: last_compile_summary.as_deref(),
                            last_audible_summary: last_audible_summary.as_deref(),
                            last_audible_ok,
                            has_compiled_spc,
                            last_sfc_compile_summary: last_sfc_compile_summary.as_deref(),
                            last_sfc_verify_summary: last_sfc_verify_summary.as_deref(),
                            last_sfc_verify_ok,
                            has_compiled_sfc,
                        },
                    );
                }
            }
        });
        if response.edited {
            if let Some(p) = self.project.as_ref() {
                self.validation_errors = p.validate().err().unwrap_or_default();
            }
            self.recompute_aram_meter();
        }
        if response.find_loops {
            self.do_find_loops();
        }
        if response.preview_brr {
            self.do_preview_brr();
        }
        if project_resp.refresh_meter {
            self.recompute_aram_meter();
        }
        if project_resp.compile_spc {
            self.do_compile_spc();
        }
        if project_resp.verify_audible {
            self.do_verify_audible();
        }
        if project_resp.compile_sfc {
            self.do_compile_sfc();
        }
        if project_resp.verify_sfc {
            self.do_verify_sfc();
        }
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

    fn do_find_loops(&mut self) {
        let Some(sample_id) = self.selected_sample_id.clone() else {
            self.status_message = Some("find loops: no sample selected".to_string());
            return;
        };
        let Some(project) = self.project.as_ref() else {
            return;
        };
        let Some(sample) = project.sample_pool.iter().find(|s| s.id == sample_id) else {
            return;
        };
        let Some(audio_path) = self.resolve_sample_audio_path(sample) else {
            self.status_message =
                Some(format!("find loops: cannot resolve {}", sample.source.path));
            return;
        };
        let pcm = match decode_to_mono_pcm(&audio_path) {
            Ok(p) => p,
            Err(e) => {
                self.status_message = Some(format!("find loops: decode failed: {e}"));
                return;
            }
        };
        let candidates = find_loop_candidates(&pcm, &LoopFinderOptions::default());
        if candidates.is_empty() {
            self.status_message =
                Some("find loops: no candidates (sample may be too short)".to_string());
            return;
        }
        self.loop_candidates_modal.target_sample_id = Some(sample_id);
        self.loop_candidates_modal.candidates = candidates;
        self.loop_candidates_modal.visible = true;
        self.status_message = Some(format!(
            "find loops: {} candidates",
            self.loop_candidates_modal.candidates.len()
        ));
    }

    fn do_preview_brr(&mut self) {
        let Some(sample_id) = self.selected_sample_id.clone() else {
            self.status_message = Some("preview: no sample selected".to_string());
            return;
        };
        let Some(project) = self.project.as_ref() else {
            return;
        };
        let Some(sample) = project
            .sample_pool
            .iter()
            .find(|s| s.id == sample_id)
            .cloned()
        else {
            return;
        };
        let Some(audio_path) = self.resolve_sample_audio_path(&sample) else {
            self.status_message = Some(format!("preview: cannot resolve {}", sample.source.path));
            return;
        };
        let pcm = match decode_to_mono_pcm(&audio_path) {
            Ok(p) => p,
            Err(e) => {
                self.status_message = Some(format!("preview: decode failed: {e}"));
                return;
            }
        };
        let opts = EncodeOptions::default();
        let encode_result = if sample.looped.enabled {
            match (sample.looped.start_sample, sample.looped.end_sample) {
                (Some(start), _) => match encode_looped(&pcm, start, &opts) {
                    Ok(r) => r,
                    Err(e) => {
                        self.status_message = Some(format!("preview: encode failed: {e}"));
                        return;
                    }
                },
                _ => brr_encode(&pcm, &opts),
            }
        } else {
            brr_encode(&pcm, &opts)
        };

        let project_dir = self
            .project_path
            .as_ref()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."));
        let preview_dir = project_dir.join(".sfcwc-preview");
        if let Err(e) = std::fs::create_dir_all(&preview_dir) {
            self.status_message = Some(format!("preview: mkdir failed: {e}"));
            return;
        }
        let wav_path = preview_dir.join(format!("{}.audition.wav", sample_id));
        let sample_rate_hz = sample.source.sample_rate_hz.max(1);
        match export_decoded_brr_wav(&encode_result.bytes, sample_rate_hz, &wav_path) {
            Ok(r) => {
                self.status_message = Some(format!(
                    "preview: wrote {} ({} samples, {} blocks; rms={:.2}, peak={})",
                    wav_path.display(),
                    r.samples_written,
                    r.blocks_decoded,
                    encode_result.summary.overall_rms_error,
                    encode_result.summary.overall_peak_error,
                ));
            }
            Err(e) => {
                self.status_message = Some(format!("preview: write failed: {e}"));
            }
        }
    }

    fn resolve_sample_audio_path(&self, sample: &SampleSlot) -> Option<PathBuf> {
        let raw = Path::new(&sample.source.path);
        if raw.is_absolute() {
            return Some(raw.to_path_buf());
        }
        let project_dir = self.project_path.as_ref()?.parent()?;
        Some(project_dir.join(raw))
    }

    fn draw_loop_candidates_modal(&mut self, ctx: &egui::Context) {
        if !self.loop_candidates_modal.visible {
            return;
        }
        let mut close = false;
        let mut apply: Option<LoopCandidate> = None;
        let target_sample = self.loop_candidates_modal.target_sample_id.clone();
        egui::Window::new("Loop candidates")
            .collapsible(false)
            .resizable(true)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(format!(
                    "Top candidates for {}:",
                    target_sample.as_deref().unwrap_or("(unknown)")
                ));
                ui.separator();
                egui::ScrollArea::vertical()
                    .max_height(360.0)
                    .show(ui, |ui| {
                        egui::Grid::new("loop_cands_grid")
                            .num_columns(5)
                            .striped(true)
                            .show(ui, |ui| {
                                ui.strong("start");
                                ui.strong("end");
                                ui.strong("rms");
                                ui.strong("click");
                                ui.label("");
                                ui.end_row();
                                for c in &self.loop_candidates_modal.candidates {
                                    ui.monospace(c.start_sample.to_string());
                                    ui.monospace(c.end_sample.to_string());
                                    ui.monospace(format!("{:.2}", c.rms_window_difference));
                                    ui.monospace(c.seam_click.to_string());
                                    if ui.button("Apply").clicked() {
                                        apply = Some(*c);
                                    }
                                    ui.end_row();
                                }
                            });
                    });
                ui.separator();
                if ui.button("Close").clicked() {
                    close = true;
                }
            });
        if let (Some(c), Some(target)) = (apply, target_sample) {
            self.apply_loop_candidate(&target, c);
            close = true;
        }
        if close {
            self.loop_candidates_modal.visible = false;
            self.loop_candidates_modal.candidates.clear();
            self.loop_candidates_modal.target_sample_id = None;
        }
    }

    /// Synchronous in-process compile-spc. On success stores the
    /// path in `last_compiled_spc` so Verify Audible can run.
    fn do_compile_spc(&mut self) {
        let Some(project) = self.project.as_ref().cloned() else {
            self.last_compile_summary = Some("compile-spc: no project loaded".to_string());
            return;
        };
        if !self.validation_errors.is_empty() {
            self.last_compile_summary =
                Some("compile-spc: project has validation errors".to_string());
            return;
        }
        let project_dir = self
            .project_path
            .as_ref()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."));

        // 1. Encode samples.
        let mut encoded: Vec<EncodedSample> = Vec::with_capacity(project.sample_pool.len());
        for slot in &project.sample_pool {
            let raw = Path::new(&slot.source.path);
            let audio_path = if raw.is_absolute() {
                raw.to_path_buf()
            } else {
                project_dir.join(raw)
            };
            let pcm = match decode_to_mono_pcm(&audio_path) {
                Ok(p) => p,
                Err(e) => {
                    self.last_compile_summary =
                        Some(format!("compile-spc: decode {}: {e}", slot.source.path));
                    return;
                }
            };
            let opts = EncodeOptions::default();
            let (bytes, loop_entry_block) = if slot.looped.enabled {
                match slot.looped.start_sample {
                    Some(start) => match encode_looped(&pcm, start, &opts) {
                        Ok(r) => (r.bytes, Some(start / 16)),
                        Err(e) => {
                            self.last_compile_summary =
                                Some(format!("compile-spc: encode_looped {}: {e}", slot.id));
                            return;
                        }
                    },
                    None => (brr_encode(&pcm, &opts).bytes, None),
                }
            } else {
                (brr_encode(&pcm, &opts).bytes, None)
            };
            encoded.push(EncodedSample {
                sample_id: slot.id.clone(),
                bytes,
                loop_entry_block,
            });
        }

        // 2. Shadow pack to get the layout for driver_build.
        let shadow = match packer_pack(PackInput {
            project: project.clone(),
            encoded_samples: encoded.clone(),
            driver_code: Vec::new(),
        }) {
            Ok(r) => r,
            Err(e) => {
                self.last_compile_summary = Some(format!("compile-spc: shadow pack: {e}"));
                return;
            }
        };

        // 3. Build driver.
        let work = match tempfile::tempdir() {
            Ok(d) => d,
            Err(e) => {
                self.last_compile_summary = Some(format!("compile-spc: tempdir: {e}"));
                return;
            }
        };
        let driver_out = match driver_build(DriverBuildInput {
            project: &project,
            map_report: &shadow.map_report,
            source_override: None,
            working_dir: work.path().to_path_buf(),
        }) {
            Ok(o) => o,
            Err(e) => {
                self.last_compile_summary = Some(format!("compile-spc: driver_build: {e}"));
                return;
            }
        };

        // 4. Real pack.
        let result = match packer_pack(PackInput {
            project: project.clone(),
            encoded_samples: encoded,
            driver_code: driver_out.driver_code.clone(),
        }) {
            Ok(r) => r,
            Err(e) => {
                self.last_compile_summary = Some(format!("compile-spc: pack: {e}"));
                return;
            }
        };

        // 5. SPC + write.
        let aram_vec: Vec<u8> = result.aram_image[..].to_vec();
        let spc = match build_m1_image(aram_vec) {
            Ok(s) => s,
            Err(e) => {
                self.last_compile_summary = Some(format!("compile-spc: build_m1_image: {e:?}"));
                return;
            }
        };
        let spc_bytes = match spc.to_bytes() {
            Ok(b) => b,
            Err(e) => {
                self.last_compile_summary = Some(format!("compile-spc: to_bytes: {e:?}"));
                return;
            }
        };

        let stem = project
            .project
            .name
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
            .collect::<String>();
        let stem = if stem.is_empty() {
            "project".to_string()
        } else {
            stem
        };
        let out_dir = project_dir.join(".sfcwc-build");
        if let Err(e) = std::fs::create_dir_all(&out_dir) {
            self.last_compile_summary =
                Some(format!("compile-spc: mkdir {}: {e}", out_dir.display()));
            return;
        }
        let spc_path = out_dir.join(format!("{stem}.spc"));
        if let Err(e) = std::fs::write(&spc_path, &spc_bytes) {
            self.last_compile_summary =
                Some(format!("compile-spc: write {}: {e}", spc_path.display()));
            return;
        }

        self.last_compiled_spc = Some(spc_path.clone());
        self.last_compile_summary = Some(format!(
            "compile-spc OK: driver={} B, spc={} B → {}",
            driver_out.driver_code.len(),
            spc_bytes.len(),
            spc_path.display(),
        ));
        self.last_audible_summary = None;
        self.last_audible_ok = None;
    }

    fn do_verify_audible(&mut self) {
        let Some(spc) = self.last_compiled_spc.clone() else {
            self.last_audible_summary = Some("verify: no compiled SPC".to_string());
            self.last_audible_ok = Some(false);
            return;
        };
        let workspace_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let oracle = resolve_snes_spc_oracle(&workspace_root);
        if !oracle.resolved {
            self.last_audible_summary =
                Some("verify: oracle wrapper not resolved (set SFCWC_SNES_SPC_ORACLE)".to_string());
            self.last_audible_ok = Some(false);
            return;
        }
        let oracle_path = oracle.path.expect("resolved => path");

        let pcm_path = spc.with_extension("audible.pcm_s16le");
        let report_path = spc.with_extension("audible-report.json");
        let frames = 16384u32;
        let output = std::process::Command::new(&oracle_path)
            .arg("render")
            .arg("--input-spc")
            .arg(&spc)
            .arg("--frames")
            .arg(frames.to_string())
            .arg("--output-pcm")
            .arg(&pcm_path)
            .arg("--report")
            .arg(&report_path)
            .output();
        let output = match output {
            Ok(o) => o,
            Err(e) => {
                self.last_audible_summary = Some(format!("verify: spawn oracle: {e}"));
                self.last_audible_ok = Some(false);
                return;
            }
        };
        if !output.status.success() {
            self.last_audible_summary = Some(format!(
                "verify: oracle exited {}",
                output.status.code().unwrap_or(-1)
            ));
            self.last_audible_ok = Some(false);
            return;
        }
        let pcm = match std::fs::read(&pcm_path) {
            Ok(b) => b,
            Err(e) => {
                self.last_audible_summary = Some(format!("verify: read PCM: {e}"));
                self.last_audible_ok = Some(false);
                return;
            }
        };
        let (max_abs, rms) = pcm_stats(&pcm);
        let ok = max_abs >= 1000 && rms >= 200.0;
        self.last_audible_summary = Some(format!(
            "verify: {} frames, max_abs={}, rms={:.1}, status={}",
            frames,
            max_abs,
            rms,
            if ok { "ok" } else { "silent_fail" },
        ));
        self.last_audible_ok = Some(ok);
    }

    /// Synchronous in-process compile-sfc (M1.6) — uses the same
    /// project as compile-spc, runs sfc_export against just project
    /// A, drops the .sfc next to the .spc under .sfcwc-build/.
    fn do_compile_sfc(&mut self) {
        let Some(project_path) = self.project_path.clone() else {
            self.last_sfc_compile_summary = Some("compile-sfc: no project path".to_string());
            return;
        };
        if !self.validation_errors.is_empty() {
            self.last_sfc_compile_summary =
                Some("compile-sfc: project has validation errors".to_string());
            return;
        }
        let project_dir = project_path
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        let stem = self
            .project
            .as_ref()
            .map(|p| p.project.name.as_str())
            .unwrap_or("project")
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
            .collect::<String>();
        let stem = if stem.is_empty() {
            "project".to_string()
        } else {
            stem
        };
        let out_dir = project_dir.join(".sfcwc-build");
        if let Err(e) = std::fs::create_dir_all(&out_dir) {
            self.last_sfc_compile_summary =
                Some(format!("compile-sfc: mkdir {}: {e}", out_dir.display()));
            return;
        }
        let out_sfc = out_dir.join(format!("{stem}.sfc"));
        let work = match tempfile::tempdir() {
            Ok(d) => d,
            Err(e) => {
                self.last_sfc_compile_summary = Some(format!("compile-sfc: tempdir: {e}"));
                return;
            }
        };
        match export_sfc(SfcExportInput {
            project_a_path: project_path.clone(),
            project_b_path: None,
            loader_source_override: None,
            working_dir: work.path().to_path_buf(),
            out_sfc_path: out_sfc.clone(),
        }) {
            Ok(r) => {
                self.last_compiled_sfc = Some(r.sfc_path.clone());
                self.last_sfc_compile_summary = Some(format!(
                    "compile-sfc OK: .sfc={} B, module={} B, loader={} B → {}",
                    r.sfc_size_bytes,
                    r.module_a.module_bytes.len(),
                    r.loader_size_bytes,
                    r.sfc_path.display(),
                ));
                self.last_sfc_verify_summary = None;
                self.last_sfc_verify_ok = None;
            }
            Err(e) => {
                self.last_sfc_compile_summary = Some(format!("compile-sfc: {e}"));
            }
        }
    }

    /// In-process verify: parse module A from the just-compiled .sfc,
    /// project blocks → ARAM → SPC → oracle render → max_abs/rms.
    fn do_verify_sfc(&mut self) {
        let Some(sfc) = self.last_compiled_sfc.clone() else {
            self.last_sfc_verify_summary = Some("verify-sfc: no compiled .sfc".to_string());
            self.last_sfc_verify_ok = Some(false);
            return;
        };
        let workspace_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let oracle = sfc_atomizer_core::tools::resolve_snes_spc_oracle(&workspace_root);
        if !oracle.resolved {
            self.last_sfc_verify_summary =
                Some("verify-sfc: oracle wrapper not resolved".to_string());
            self.last_sfc_verify_ok = Some(false);
            return;
        }
        let oracle_path = oracle.path.expect("resolved => path");

        let bytes = match std::fs::read(&sfc) {
            Ok(b) => b,
            Err(e) => {
                self.last_sfc_verify_summary = Some(format!("verify-sfc: read sfc: {e}"));
                self.last_sfc_verify_ok = Some(false);
                return;
            }
        };
        if MODULE_A_FILE_OFFSET + 64 > bytes.len() {
            self.last_sfc_verify_summary =
                Some("verify-sfc: .sfc too small to embed module A".to_string());
            self.last_sfc_verify_ok = Some(false);
            return;
        }
        let mod_total_len = u32::from_le_bytes(
            bytes[MODULE_A_FILE_OFFSET + 0x18..MODULE_A_FILE_OFFSET + 0x1C]
                .try_into()
                .unwrap_or([0; 4]),
        );
        let mod_end = MODULE_A_FILE_OFFSET + mod_total_len as usize;
        if mod_end > bytes.len() {
            self.last_sfc_verify_summary = Some("verify-sfc: module A runs past file".to_string());
            self.last_sfc_verify_ok = Some(false);
            return;
        }
        let module_slice = &bytes[MODULE_A_FILE_OFFSET..mod_end];
        let header = match parse_module_header(module_slice) {
            Ok(h) => h,
            Err(e) => {
                self.last_sfc_verify_summary = Some(format!("verify-sfc: parse header: {e}"));
                self.last_sfc_verify_ok = Some(false);
                return;
            }
        };
        let blocks = match parse_module_blocks(module_slice, &header) {
            Ok(b) => b,
            Err(e) => {
                self.last_sfc_verify_summary = Some(format!("verify-sfc: parse blocks: {e}"));
                self.last_sfc_verify_ok = Some(false);
                return;
            }
        };
        let aram = project_blocks_to_aram(module_slice, &header, &blocks);
        let spc = match sfc_atomizer_core::spc::build_m1_image(aram.to_vec()) {
            Ok(s) => s,
            Err(e) => {
                self.last_sfc_verify_summary = Some(format!("verify-sfc: build_m1_image: {e:?}"));
                self.last_sfc_verify_ok = Some(false);
                return;
            }
        };
        let spc_bytes = match spc.to_bytes() {
            Ok(b) => b,
            Err(e) => {
                self.last_sfc_verify_summary = Some(format!("verify-sfc: spc to_bytes: {e:?}"));
                self.last_sfc_verify_ok = Some(false);
                return;
            }
        };
        let work = match tempfile::tempdir() {
            Ok(d) => d,
            Err(e) => {
                self.last_sfc_verify_summary = Some(format!("verify-sfc: tempdir: {e}"));
                self.last_sfc_verify_ok = Some(false);
                return;
            }
        };
        let spc_path = work.path().join("module_a.spc");
        if let Err(e) = std::fs::write(&spc_path, &spc_bytes) {
            self.last_sfc_verify_summary = Some(format!("verify-sfc: write spc: {e}"));
            self.last_sfc_verify_ok = Some(false);
            return;
        }
        let pcm_path = work.path().join("module_a.pcm");
        let report_path = work.path().join("oracle.json");
        let frames = 16384u32;
        let output = std::process::Command::new(&oracle_path)
            .arg("render")
            .arg("--input-spc")
            .arg(&spc_path)
            .arg("--frames")
            .arg(frames.to_string())
            .arg("--output-pcm")
            .arg(&pcm_path)
            .arg("--report")
            .arg(&report_path)
            .output();
        let output = match output {
            Ok(o) => o,
            Err(e) => {
                self.last_sfc_verify_summary = Some(format!("verify-sfc: spawn oracle: {e}"));
                self.last_sfc_verify_ok = Some(false);
                return;
            }
        };
        if !output.status.success() {
            self.last_sfc_verify_summary = Some(format!(
                "verify-sfc: oracle exited {}",
                output.status.code().unwrap_or(-1)
            ));
            self.last_sfc_verify_ok = Some(false);
            return;
        }
        let pcm = match std::fs::read(&pcm_path) {
            Ok(b) => b,
            Err(e) => {
                self.last_sfc_verify_summary = Some(format!("verify-sfc: read pcm: {e}"));
                self.last_sfc_verify_ok = Some(false);
                return;
            }
        };
        let (max_abs, rms) = pcm_stats(&pcm);
        let ok = max_abs >= 1000 && rms >= 200.0;
        self.last_sfc_verify_summary = Some(format!(
            "verify-sfc: module A max_abs={}, rms={:.1}, status={}",
            max_abs,
            rms,
            if ok { "ok" } else { "silent_fail" }
        ));
        self.last_sfc_verify_ok = Some(ok);
    }

    fn apply_loop_candidate(&mut self, sample_id: &str, c: LoopCandidate) {
        let Some(project) = self.project.as_mut() else {
            return;
        };
        let Some(sample) = project.sample_pool.iter_mut().find(|s| s.id == sample_id) else {
            return;
        };
        sample.looped.enabled = true;
        sample.looped.start_sample = Some(c.start_sample);
        sample.looped.end_sample = Some(c.end_sample);
        if let Some(p) = self.project.as_ref() {
            self.validation_errors = p.validate().err().unwrap_or_default();
        }
        self.recompute_aram_meter();
        self.status_message = Some(format!(
            "loop applied: start={} end={}",
            c.start_sample, c.end_sample
        ));
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

#[derive(Default, Clone, Copy)]
struct ProjectDetailResponse {
    refresh_meter: bool,
    compile_spc: bool,
    verify_audible: bool,
    compile_sfc: bool,
    verify_sfc: bool,
}

/// State the project-detail panel reads from `SfcwcApp`. Cloned
/// into a single struct so the rendering function stays under
/// clippy's 7-arg ceiling.
struct ProjectDetailState<'a> {
    project: &'a ProjectV1,
    aram_meter: Option<&'a AramMapReport>,
    aram_meter_error: Option<&'a str>,
    last_compile_summary: Option<&'a str>,
    last_audible_summary: Option<&'a str>,
    last_audible_ok: Option<bool>,
    has_compiled_spc: bool,
    last_sfc_compile_summary: Option<&'a str>,
    last_sfc_verify_summary: Option<&'a str>,
    last_sfc_verify_ok: Option<bool>,
    has_compiled_sfc: bool,
}

fn draw_project_detail(ui: &mut egui::Ui, s: ProjectDetailState<'_>) -> ProjectDetailResponse {
    let ProjectDetailState {
        project,
        aram_meter,
        aram_meter_error,
        last_compile_summary,
        last_audible_summary,
        last_audible_ok,
        has_compiled_spc,
        last_sfc_compile_summary,
        last_sfc_verify_summary,
        last_sfc_verify_ok,
        has_compiled_sfc,
    } = s;
    let mut resp = ProjectDetailResponse::default();
    egui::ScrollArea::vertical().show(ui, |ui| {
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

        ui.add_space(12.0);
        ui.separator();
        ui.horizontal(|ui| {
            ui.heading("ARAM meter");
            if ui.button("Refresh").clicked() {
                resp.refresh_meter = true;
            }
        });
        if let Some(err) = aram_meter_error {
            ui.colored_label(egui::Color32::from_rgb(220, 80, 80), format!("⚠ {err}"));
        }
        if let Some(report) = aram_meter {
            draw_aram_meter(ui, report);
        } else if aram_meter_error.is_none() {
            ui.weak("(meter unavailable — open a valid project)");
        }

        ui.add_space(12.0);
        ui.separator();
        ui.heading("Compile + verify (M1.5)");
        ui.horizontal(|ui| {
            if ui.button("Compile SPC").clicked() {
                resp.compile_spc = true;
            }
            if ui
                .add_enabled(has_compiled_spc, egui::Button::new("Verify Audible"))
                .clicked()
            {
                resp.verify_audible = true;
            }
        });
        if let Some(s) = last_compile_summary {
            ui.weak(s);
        }
        if let Some(s) = last_audible_summary {
            let color = match last_audible_ok {
                Some(true) => egui::Color32::from_rgb(80, 180, 100),
                Some(false) => egui::Color32::from_rgb(220, 80, 80),
                None => egui::Color32::GRAY,
            };
            ui.colored_label(color, s);
        }

        ui.add_space(12.0);
        ui.separator();
        ui.heading("Compile + verify (M1.6 — .sfc)");
        ui.horizontal(|ui| {
            if ui.button("Compile SFC").clicked() {
                resp.compile_sfc = true;
            }
            if ui
                .add_enabled(has_compiled_sfc, egui::Button::new("Verify SFC"))
                .clicked()
            {
                resp.verify_sfc = true;
            }
        });
        if let Some(s) = last_sfc_compile_summary {
            ui.weak(s);
        }
        if let Some(s) = last_sfc_verify_summary {
            let color = match last_sfc_verify_ok {
                Some(true) => egui::Color32::from_rgb(80, 180, 100),
                Some(false) => egui::Color32::from_rgb(220, 80, 80),
                None => egui::Color32::GRAY,
            };
            ui.colored_label(color, s);
        }
    });
    resp
}

fn draw_aram_meter(ui: &mut egui::Ui, report: &AramMapReport) {
    let total = report.total_aram as f32;
    let bar_height = 24.0;
    let avail_width = ui.available_width().min(720.0);

    let (rect, _resp) =
        ui.allocate_exact_size(egui::vec2(avail_width, bar_height), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2.0, egui::Color32::from_gray(40));

    let mut x = rect.left();
    for region in &report.regions {
        let width = (region.bytes as f32 / total) * avail_width;
        if width <= 0.0 {
            continue;
        }
        let segment =
            egui::Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(width, bar_height));
        painter.rect_filled(segment, 0.0, color_for_kind(region.kind));
        x += width;
    }
    painter.rect_stroke(
        rect,
        2.0,
        egui::Stroke::new(1.0, egui::Color32::from_gray(120)),
        egui::StrokeKind::Inside,
    );

    ui.add_space(8.0);

    // Echo callout.
    if let Some(echo) = report.echo.as_ref() {
        if echo.enabled {
            let label = format!(
                "Echo: {} B ({:.1}% of ARAM), EDL={}, ESA=$ {:02X}",
                echo.buffer_bytes, echo.percent_of_aram, echo.edl, echo.esa,
            );
            if echo.writeback_safe {
                ui.label(egui::RichText::new(label).strong());
            } else {
                ui.colored_label(
                    egui::Color32::from_rgb(220, 80, 80),
                    format!("⚠ {label}  (writeback hazard)"),
                );
            }
        } else {
            ui.label(egui::RichText::new("Echo: 0 B; echo writeback disabled").weak());
        }
    }

    ui.add_space(8.0);
    egui::Grid::new("aram_meter_breakdown")
        .num_columns(2)
        .show(ui, |ui| {
            ui.label("total");
            ui.monospace(format!("{} B", report.total_aram));
            ui.end_row();
            for region in &report.regions {
                ui.label(&region.name);
                ui.monospace(format!(
                    "{}–{}  ({} B)",
                    region.start, region.end, region.bytes,
                ));
                ui.end_row();
            }
            ui.label("free_bytes");
            ui.monospace(format!("{} B", report.free_bytes));
            ui.end_row();
        });

    if !report.warnings.is_empty() {
        ui.add_space(8.0);
        ui.label(egui::RichText::new("Warnings:").strong());
        for w in &report.warnings {
            ui.colored_label(egui::Color32::from_rgb(220, 200, 80), format!("• {w}"));
        }
    }
}

fn pcm_stats(pcm: &[u8]) -> (i32, f64) {
    let n = pcm.len() / 2;
    if n == 0 {
        return (0, 0.0);
    }
    let mut max_abs: i32 = 0;
    let mut sum_sq: f64 = 0.0;
    for chunk in pcm.chunks_exact(2) {
        let s = i16::from_le_bytes([chunk[0], chunk[1]]) as i32;
        let a = s.unsigned_abs() as i32;
        if a > max_abs {
            max_abs = a;
        }
        sum_sq += (s as f64) * (s as f64);
    }
    (max_abs, (sum_sq / n as f64).sqrt())
}

fn color_for_kind(kind: AramKind) -> egui::Color32 {
    match kind {
        AramKind::FixedRuntime | AramKind::FixedHardware => egui::Color32::from_gray(80),
        AramKind::DriverCode => egui::Color32::from_rgb(160, 160, 170),
        AramKind::SourceDirectory => egui::Color32::from_rgb(150, 100, 200),
        AramKind::SampleBrrPool => egui::Color32::from_rgb(80, 130, 220),
        AramKind::EchoBuffer => egui::Color32::from_rgb(230, 140, 60),
        AramKind::Free => egui::Color32::from_rgb(80, 180, 100),
        AramKind::PitchTables
        | AramKind::SequenceData
        | AramKind::InstrumentMetadata
        | AramKind::SynthAtomPool => egui::Color32::from_rgb(180, 180, 80),
    }
}

#[derive(Default, Clone, Copy)]
struct SampleDetailResponse {
    edited: bool,
    find_loops: bool,
    preview_brr: bool,
}

fn draw_sample_detail(ui: &mut egui::Ui, s: &mut SampleSlot) -> SampleDetailResponse {
    let mut resp = SampleDetailResponse::default();
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
                if ui.checkbox(&mut s.looped.enabled, "").changed() {
                    resp.edited = true;
                }
                ui.end_row();

                if s.looped.enabled {
                    let frames = s.source.frames as u32;
                    ui.label("loop.start_sample");
                    let mut start = s.looped.start_sample.unwrap_or(0);
                    if ui
                        .add(
                            egui::DragValue::new(&mut start)
                                .speed(16.0)
                                .range(0..=frames.saturating_sub(1)),
                        )
                        .changed()
                    {
                        s.looped.start_sample = Some(start - (start % 16));
                        resp.edited = true;
                    }
                    ui.end_row();

                    ui.label("loop.end_sample");
                    let mut end = s.looped.end_sample.unwrap_or(frames);
                    if ui
                        .add(egui::DragValue::new(&mut end).speed(16.0).range(0..=frames))
                        .changed()
                    {
                        s.looped.end_sample = Some(end - (end % 16));
                        resp.edited = true;
                    }
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
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("Find Loop Candidates").clicked() {
                resp.find_loops = true;
            }
            if ui.button("Preview BRR").clicked() {
                resp.preview_brr = true;
            }
        });
    });
    resp
}

fn format_format(f: &sfc_atomizer_core::project::SampleFormat) -> &'static str {
    use sfc_atomizer_core::project::SampleFormat;
    match f {
        SampleFormat::Wav => "wav",
        SampleFormat::Aiff => "aiff",
        SampleFormat::Brr => "brr",
    }
}

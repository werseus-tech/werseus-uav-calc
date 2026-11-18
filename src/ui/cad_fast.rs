// SPDX-License-Identifier: AGPL-3.0-or-later
//! Fast CAD backend (build123d) controls for the Outputs step and the CAD settings page:
//! backend choice, output formats, Python detection status and the configurable Python path.
//! Logic lives in `crate::cad::build123d`; this is UI only.

use super::theme;
use crate::cad::build123d::{
    BUILD123D_INSTALL_HINT, CadBackend, FastFormats, PythonDetection, PythonInstallation,
    detect_python_async,
};
use crate::i18n::{t, tf};
use egui::{RichText, Ui};
use std::sync::mpsc::Receiver;

/// Fast-backend UI state (part of `OutputsUi`).
#[derive(Default)]
pub struct FastUi {
    pub backend: CadBackend,
    pub formats: FastFormats,
    /// Configured interpreter (settings); empty: search PATH and the `py` launcher.
    pub python_path: String,
    path_loaded: bool,
    pub detection: Option<PythonDetection>,
    rx: Option<Receiver<PythonDetection>>,
}

impl FastUi {
    /// Reads the configured Python path from the settings file once.
    pub fn ensure_path_loaded(&mut self) {
        if !self.path_loaded {
            self.path_loaded = true;
            if self.python_path.trim().is_empty() {
                self.python_path = crate::i18n::load_settings()
                    .python_cmd_path
                    .unwrap_or_default();
            }
        }
    }

    /// Starts a background search (never blocks the frame).
    pub fn detect(&mut self, ctx: Option<egui::Context>) {
        self.ensure_path_loaded();
        let cfg = Some(self.python_path.trim().to_string()).filter(|s| !s.is_empty());
        self.rx = Some(detect_python_async(cfg, ctx));
    }

    pub fn detecting(&self) -> bool {
        self.rx.is_some()
    }

    pub fn poll(&mut self) {
        if let Some(rx) = &self.rx {
            match rx.try_recv() {
                Ok(d) => {
                    self.detection = Some(d);
                    self.rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.detection = Some(PythonDetection::default());
                    self.rx = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
        }
    }

    pub fn found(&self) -> Option<&PythonInstallation> {
        self.detection.as_ref().and_then(|d| d.found.as_ref())
    }
}

/// Backend radio buttons and, for the fast backend, the format checkboxes.
pub fn backend_controls(ui: &mut Ui, st: &mut FastUi, running: bool) {
    ui.strong(t("cadfast.backend_label"));
    ui.add_enabled_ui(!running, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.radio_value(&mut st.backend, CadBackend::Fast, t("cadfast.backend_fast"))
                .on_hover_text(t("cadfast.backend_fast_tip"));
            ui.radio_value(
                &mut st.backend,
                CadBackend::FreeCad,
                t("cadfast.backend_freecad"),
            )
            .on_hover_text(t("cadfast.backend_freecad_tip"));
        });
        if st.backend == CadBackend::Fast {
            ui.label(RichText::new(t("cadfast.intro")).weak());
            ui.horizontal_wrapped(|ui| {
                ui.label(t("cadfast.formats_label"));
                ui.checkbox(&mut st.formats.step, t("cadfast.fmt_step"));
                ui.checkbox(&mut st.formats.stl, t("cadfast.fmt_stl"));
                ui.checkbox(&mut st.formats.dxf, t("cadfast.fmt_dxf"))
                    .on_hover_text(t("cadfast.fmt_dxf_tip"));
            });
        }
    });
}

/// Python / build123d status for the Outputs step; `true` when generation can start.
pub fn fast_status(ui: &mut Ui, st: &mut FastUi, running: bool) -> bool {
    st.poll();
    // Tests never spawn interpreters from a frame.
    if st.detection.is_none() && !st.detecting() && !running && !cfg!(test) {
        st.detect(Some(ui.ctx().clone()));
    }
    let tk = theme::tokens(ui);
    if st.detecting() {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(t("cadfast.detecting"));
        });
        ui.ctx().request_repaint();
    } else if let Some(py) = st.found() {
        ui.colored_label(
            tk.success,
            tf(
                "cadfast.python_found_fmt",
                &[
                    py.python_version.clone(),
                    py.build123d_version.clone(),
                    py.display(),
                ],
            ),
        );
    } else if st.detection.is_some() {
        ui.colored_label(tk.warning, t("cadfast.python_missing"));
        ui.horizontal(|ui| {
            ui.label(t("cadfast.install_hint_label"));
            ui.label(RichText::new(BUILD123D_INSTALL_HINT).monospace().strong());
            if ui
                .small_button("⧉")
                .on_hover_text(t("cadfast.copy_hint"))
                .clicked()
            {
                ui.ctx().copy_text(BUILD123D_INSTALL_HINT.to_string());
            }
        });
        if let Some(d) = &st.detection {
            if !d.without_build123d.is_empty() {
                ui.label(
                    RichText::new(tf(
                        "cadfast.python_without_fmt",
                        &[d.without_build123d.join(", ")],
                    ))
                    .weak(),
                );
            }
        }
        ui.horizontal(|ui| {
            if ui.button(t("cadfast.btn_use_freecad")).clicked() {
                st.backend = CadBackend::FreeCad;
            }
            if ui
                .add_enabled(!running, egui::Button::new(t("cadfast.btn_redetect")))
                .clicked()
            {
                st.detect(Some(ui.ctx().clone()));
            }
        });
    }
    if !st.formats.any() {
        ui.colored_label(tk.warning, t("cadfast.no_formats"));
    }
    st.found().is_some() && st.formats.any() && !st.detecting()
}

/// Python path row for Settings > CAD (saved with the other CAD settings).
pub fn settings_rows(ui: &mut Ui, st: &mut FastUi) {
    st.ensure_path_loaded();
    st.poll();
    ui.label(RichText::new(t("cadfast.python_desc")).weak());
    ui.horizontal(|ui| {
        ui.label(t("cadfast.python_path_label"));
        ui.add(
            egui::TextEdit::singleline(&mut st.python_path)
                .desired_width((ui.available_width() - 90.0).max(120.0)),
        );
        if ui.button(t("settings.btn_browse")).clicked() {
            if let Some(p) = rfd::FileDialog::new()
                .add_filter("Python", &["exe", "*"])
                .pick_file()
            {
                st.python_path = p.display().to_string();
            }
        }
    });
    if ui
        .add_enabled(
            !st.detecting(),
            egui::Button::new(t("cadfast.btn_redetect")),
        )
        .clicked()
    {
        st.detect(Some(ui.ctx().clone()));
    }
    let tk = theme::tokens(ui);
    if st.detecting() {
        ui.label(t("cadfast.detecting"));
    } else if let Some(py) = st.found() {
        ui.colored_label(
            tk.success,
            tf(
                "cadfast.python_found_fmt",
                &[
                    py.python_version.clone(),
                    py.build123d_version.clone(),
                    py.display(),
                ],
            ),
        );
    } else if st.detection.is_some() {
        ui.colored_label(tk.warning, t("cadfast.python_missing"));
        ui.label(RichText::new(BUILD123D_INSTALL_HINT).monospace());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::{Lang, with_lang};
    use crate::ui::theme::{self, ThemeChoice};

    fn render(st: &mut FastUi, running: bool) -> bool {
        let ctx = egui::Context::default();
        theme::install(&ctx, ThemeChoice::Dark);
        let mut ready = false;
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                backend_controls(ui, st, running);
                ready = fast_status(ui, st, running);
                settings_rows(ui, st);
            });
        });
        out.textures_delta.clear();
        ready
    }

    #[test]
    fn defaults_are_fast_backend_with_all_formats() {
        let st = FastUi::default();
        assert_eq!(st.backend, CadBackend::Fast);
        assert!(st.formats.step && st.formats.stl && st.formats.dxf);
    }

    #[test]
    fn ready_only_with_build123d_and_a_format() {
        for lang in [Lang::Tr, Lang::En] {
            with_lang(lang, || {
                let mut st = FastUi {
                    path_loaded: true,
                    ..Default::default()
                };
                // Missing: not ready, never auto-detects in tests.
                st.detection = Some(PythonDetection {
                    found: None,
                    without_build123d: vec!["C:/Python315/python.exe (3.15.2)".into()],
                });
                assert!(!render(&mut st, false));
                assert!(!st.detecting());
                st.detection = Some(PythonDetection {
                    found: Some(PythonInstallation {
                        program: "python".into(),
                        args: Vec::new(),
                        python_version: "3.12.1".into(),
                        build123d_version: "0.13.0".into(),
                    }),
                    without_build123d: Vec::new(),
                });
                assert!(render(&mut st, false));
                st.formats = FastFormats {
                    step: false,
                    stl: false,
                    dxf: false,
                };
                assert!(!render(&mut st, false));
            });
        }
    }

    #[test]
    fn missing_message_carries_the_install_hint_in_both_languages() {
        for lang in [Lang::Tr, Lang::En] {
            with_lang(lang, || {
                let m = t("cadfast.python_missing");
                assert!(m.contains("build123d"), "{m}");
                assert!(t("cadfast.backend_fast").contains("STEP"));
            });
        }
        assert_eq!(BUILD123D_INSTALL_HINT, "pip install build123d");
    }
}

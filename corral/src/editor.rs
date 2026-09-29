//! The plugin window: Key, Scale, Glide and Mix controls, the harmony
//! ("D4 → F4") and, under it, the detected note.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use nice_plug::context::gui::GuiContext;
use nice_plug::editor::dpi::LogicalSize;
use nice_plug_egui::{EguiEditorState, NiceEguiApp, widgets};

use corral_dsp::note;

use crate::CorralParams;

/// Fixed and not saved with the set.
const WINDOW_SIZE: LogicalSize<f32> = LogicalSize::new(300.0, 340.0);

pub(crate) fn default_state() -> Arc<EguiEditorState> {
    EguiEditorState::from_size(WINDOW_SIZE, 1.0)
}

pub struct CorralEditor {
    params: Arc<CorralParams>,
    /// The packed note from the last analysis hop (`note::pack`).
    note_readout: Arc<AtomicU32>,
    /// The packed (stable note, harmony note) from the last hop (`note::pack_pair`).
    harmony_readout: Arc<AtomicU32>,
    /// Set each time the window opens. nice-plug-egui 0.5.1 never calls
    /// `editor_closed`, so nothing clears it, and nothing needs to.
    gui_context: Option<GuiContext>,
}

impl CorralEditor {
    pub(crate) fn new(
        params: Arc<CorralParams>,
        note_readout: Arc<AtomicU32>,
        harmony_readout: Arc<AtomicU32>,
    ) -> Self {
        Self {
            params,
            note_readout,
            harmony_readout,
            gui_context: None,
        }
    }
}

impl NiceEguiApp for CorralEditor {
    fn build(
        &mut self,
        _egui_ctx: egui::Context,
        nice_gui_ctx: GuiContext,
        _frame: &mut nice_plug_egui::Frame,
    ) -> Result<(), nice_plug_egui::baseview::HandlerError> {
        self.gui_context = Some(nice_gui_ctx);
        Ok(())
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut nice_plug_egui::Frame) {
        let Some(gui_context) = &self.gui_context else {
            return;
        };
        let setter = gui_context.param_setter();
        let note = note::unpack(self.note_readout.load(Ordering::Relaxed))
            .map_or_else(|| String::from("—"), note::format_note);
        let harmony = note::unpack_pair(self.harmony_readout.load(Ordering::Relaxed)).map_or_else(
            || String::from("—"),
            |(stable, harmony)| {
                format!(
                    "{} → {}",
                    note::format_name(stable),
                    note::format_name(harmony)
                )
            },
        );

        // The root Ui has no background of its own.
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("Corral");
            // Goes through the host's parameter setter, so edits are recorded as automation.
            ui.add(widgets::ParamSlider::for_param(&self.params.key, &setter));
            ui.add(widgets::ParamSlider::for_param(&self.params.scale, &setter));
            ui.add(widgets::ParamSlider::for_param(&self.params.glide, &setter));
            ui.add(widgets::ParamSlider::for_param(&self.params.mix, &setter));
            ui.label(egui::RichText::new(harmony).size(28.0).monospace());
            // The raw detection, so the stabiliser can be compared with it.
            ui.label(egui::RichText::new(note).size(14.0).monospace());
        });
    }
}

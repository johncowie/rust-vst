//! The plugin window: a Gain slider and a readout of the gain being applied.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use nice_plug::context::gui::GuiContext;
use nice_plug::editor::dpi::LogicalSize;
use nice_plug::prelude::*;
use nice_plug_egui::{EguiEditorState, NiceEguiApp, widgets};

use corral_dsp::gain;

use crate::CorralParams;

/// Fixed and not saved with the set.
const WINDOW_SIZE: LogicalSize<f32> = LogicalSize::new(300.0, 120.0);

pub(crate) fn default_state() -> Arc<EguiEditorState> {
    EguiEditorState::from_size(WINDOW_SIZE, 1.0)
}

pub struct CorralEditor {
    params: Arc<CorralParams>,
    /// The smoothed gain the audio thread applied to its last frame.
    readout_gain: Arc<AtomicF32>,
    /// Set each time the window opens. nice-plug-egui 0.5.1 never calls
    /// `editor_closed`, so nothing clears it, and nothing needs to.
    gui_context: Option<GuiContext>,
}

impl CorralEditor {
    pub(crate) fn new(params: Arc<CorralParams>, readout_gain: Arc<AtomicF32>) -> Self {
        Self {
            params,
            readout_gain,
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
        let readout = gain::format_gain(self.readout_gain.load(Ordering::Relaxed));

        // The root Ui has no background of its own.
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("Corral");
            // Goes through the host's parameter setter, so edits are recorded as automation.
            ui.add(widgets::ParamSlider::for_param(&self.params.gain, &setter));
            ui.label(format!("Gain: {readout} dB"));
        });
    }
}

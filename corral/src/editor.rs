//! The plugin window: the harmony grid, Glide and Mix, and the readout
//! ("C4 → G3 · E4") with the detected note under it.
//!
//! The grid has one row per pitch class of the stable note (C at the top)
//! and one column per interval, −12 … +12. Every cell is labelled with the
//! note it would sound. The centre column (0) is the stable note itself:
//! display only, lit while that pitch class is sung. The other 288 cells
//! toggle voices in the shared grid; the sung row's "on" cells are drawn as
//! sounding.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use egui::{Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
use nice_plug::context::gui::GuiContext;
use nice_plug::editor::dpi::LogicalSize;
use nice_plug_egui::{EguiEditorState, NiceEguiApp, widgets};

use corral_dsp::grid::{self, PITCH_CLASSES, SharedGrid};
use corral_dsp::note;

use crate::CorralParams;

/// Fixed and not saved with the set.
const WINDOW_SIZE: LogicalSize<f32> = LogicalSize::new(960.0, 560.0);

/// One cell's side, and the gap between cells: 25 columns take 846 px.
const CELL: f32 = 30.0;
const GAP: f32 = 4.0;
const COLUMNS: usize = 25;
const CORNER: f32 = 4.0;

const OFF_FILL: Color32 = Color32::from_rgb(38, 40, 46);
const OFF_STROKE: Color32 = Color32::from_rgb(58, 61, 68);
const HOVER_STROKE: Color32 = Color32::from_rgb(140, 145, 155);
const OFF_TEXT: Color32 = Color32::from_rgb(150, 153, 160);
const ON_FILL: Color32 = Color32::from_rgb(40, 110, 215);
const SOUNDING_FILL: Color32 = Color32::from_rgb(75, 155, 255);
const ON_TEXT: Color32 = Color32::WHITE;
/// The stable note: its centre cell, the sounding cells' stroke and the ruler's 0.
const STABLE: Color32 = Color32::from_rgb(232, 186, 40);
const STABLE_TEXT: Color32 = Color32::from_rgb(28, 28, 30);
const CENTRE_FILL: Color32 = Color32::from_rgb(24, 25, 29);
const CENTRE_OUTLINE: Color32 = Color32::from_rgb(170, 173, 180);
const SOUNDING_BAND: Color32 = Color32::from_rgb(44, 48, 58);
const LABEL_TEXT: Color32 = Color32::from_rgb(160, 163, 170);

pub(crate) fn default_state() -> Arc<EguiEditorState> {
    EguiEditorState::from_size(WINDOW_SIZE, 1.0)
}

pub struct CorralEditor {
    params: Arc<CorralParams>,
    /// Toggled here, copied by the audio thread once per buffer.
    grid: Arc<SharedGrid>,
    /// The packed note from the last analysis hop (`note::pack`).
    note_readout: Arc<AtomicU32>,
    /// The packed stable note from the last hop (`note::pack_midi`).
    stable_readout: Arc<AtomicU32>,
    /// Set each time the window opens. nice-plug-egui 0.5.1 never calls
    /// `editor_closed`, so nothing clears it, and nothing needs to.
    gui_context: Option<GuiContext>,
}

impl CorralEditor {
    pub(crate) fn new(
        params: Arc<CorralParams>,
        grid: Arc<SharedGrid>,
        note_readout: Arc<AtomicU32>,
        stable_readout: Arc<AtomicU32>,
    ) -> Self {
        Self {
            params,
            grid,
            note_readout,
            stable_readout,
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
        let detected = note::unpack(self.note_readout.load(Ordering::Relaxed))
            .map_or_else(|| String::from("—"), note::format_note);
        let stable = note::unpack_midi(self.stable_readout.load(Ordering::Relaxed));
        let sounding_pc = stable.map(|n| n.rem_euclid(12) as usize);
        let readout = stable.map_or_else(
            || String::from("—"),
            |n| grid::format_voices(n, self.grid.row(n.rem_euclid(12) as usize)),
        );

        // The root Ui has no background of its own.
        egui::CentralPanel::default().show(ui, |ui| {
            let area = ui.max_rect();
            let width = COLUMNS as f32 * CELL + (COLUMNS - 1) as f32 * GAP;
            let left = area.center().x - width / 2.0;
            let right = left + width;
            let painter = ui.painter().clone();

            // Title line: the name on the left, the legend on the right.
            let title_y = area.top() + 16.0;
            painter.text(
                Pos2::new(left, title_y),
                Align2::LEFT_CENTER,
                "Corral",
                FontId::proportional(24.0),
                Color32::WHITE,
            );
            legend(&painter, right, title_y);

            // Ruler: one label per column, the 0 in the stable colour.
            let ruler_y = area.top() + 48.0;
            for column in 0..COLUMNS {
                let interval = column as i32 - 12;
                let text = if interval > 0 {
                    format!("+{interval}")
                } else {
                    interval.to_string()
                };
                painter.text(
                    Pos2::new(column_x(left, column) + CELL / 2.0, ruler_y),
                    Align2::CENTER_CENTER,
                    text,
                    FontId::proportional(12.0),
                    if interval == 0 { STABLE } else { LABEL_TEXT },
                );
            }

            let top = area.top() + 66.0;
            let row_y = |pc: usize| top + pc as f32 * (CELL + GAP);
            let bottom = row_y(PITCH_CLASSES - 1) + CELL;
            if let Some(pc) = sounding_pc {
                let band = Rect::from_min_max(
                    Pos2::new(left, row_y(pc)),
                    Pos2::new(right, row_y(pc) + CELL),
                );
                painter.rect_filled(band.expand(GAP / 2.0 + 1.0), CORNER + 2.0, SOUNDING_BAND);
            }

            for pc in 0..PITCH_CLASSES {
                let sounding = sounding_pc == Some(pc);
                for column in 0..COLUMNS {
                    let interval = column as i8 - 12;
                    let rect = Rect::from_min_size(
                        Pos2::new(column_x(left, column), row_y(pc)),
                        Vec2::splat(CELL),
                    );
                    let name = grid::cell_name(pc, interval);
                    if interval == 0 {
                        centre_cell(&painter, rect, name, sounding);
                        continue;
                    }
                    let response = ui.allocate_rect(rect, Sense::click());
                    if response.clicked() {
                        self.grid.toggle(pc, interval);
                    }
                    // Read after the click, so the cell changes on this frame.
                    let on = self.grid.row(pc).contains(interval);
                    toggle_cell(&painter, rect, name, on, on && sounding, response.hovered());
                }
            }

            // One outline around the whole centre column.
            let centre_x = column_x(left, 12);
            let outline =
                Rect::from_min_max(Pos2::new(centre_x, top), Pos2::new(centre_x + CELL, bottom))
                    .expand(GAP / 2.0 + 0.5);
            painter.rect_stroke(
                outline,
                CORNER + 2.0,
                Stroke::new(1.5, CENTRE_OUTLINE),
                StrokeKind::Middle,
            );

            // Bottom strip: the sliders on the left, the readout on the right.
            let strip_top = bottom + 22.0;
            let sliders = Rect::from_min_max(
                Pos2::new(left, strip_top),
                Pos2::new(left + 420.0, area.bottom()),
            );
            ui.scope_builder(egui::UiBuilder::new().max_rect(sliders), |ui| {
                egui::Grid::new("sliders")
                    .spacing(Vec2::new(16.0, 8.0))
                    .show(ui, |ui| {
                        // Goes through the host's parameter setter, so edits
                        // are recorded as automation.
                        ui.label("Glide");
                        ui.add(
                            widgets::ParamSlider::for_param(&self.params.glide, &setter)
                                .with_width(340.0),
                        );
                        ui.end_row();
                        ui.label("Mix");
                        ui.add(
                            widgets::ParamSlider::for_param(&self.params.mix, &setter)
                                .with_width(340.0),
                        );
                        ui.end_row();
                    });
            });

            let readout_left = left + 480.0;
            let readout_font = fitted_mono(&painter, &readout, 28.0, right - readout_left);
            painter.text(
                Pos2::new(readout_left, strip_top),
                Align2::LEFT_TOP,
                readout,
                readout_font,
                Color32::WHITE,
            );
            // The raw detection, so the stabiliser can be compared with it.
            painter.text(
                Pos2::new(readout_left, strip_top + 36.0),
                Align2::LEFT_TOP,
                detected,
                FontId::monospace(14.0),
                LABEL_TEXT,
            );
        });
    }
}

fn column_x(left: f32, column: usize) -> f32 {
    left + column as f32 * (CELL + GAP)
}

/// The legend, right-aligned to `right`: a swatch and a label for each
/// kind of cell.
fn legend(painter: &egui::Painter, right: f32, y: f32) {
    const SWATCH: f32 = 14.0;
    const ITEMS: [(&str, Color32, Option<Color32>); 4] = [
        ("stable note", STABLE, None),
        ("on", ON_FILL, None),
        ("on and sounding", SOUNDING_FILL, Some(STABLE)),
        ("off", OFF_FILL, Some(OFF_STROKE)),
    ];
    let font = FontId::proportional(13.0);
    let galleys: Vec<_> = ITEMS
        .iter()
        .map(|(label, ..)| painter.layout_no_wrap((*label).to_owned(), font.clone(), LABEL_TEXT))
        .collect();
    let item_width = |i: usize| SWATCH + 6.0 + galleys[i].size().x;
    let total: f32 =
        (0..ITEMS.len()).map(item_width).sum::<f32>() + 18.0 * (ITEMS.len() - 1) as f32;
    let mut x = right - total;
    for (i, (_, fill, stroke)) in ITEMS.iter().enumerate() {
        let swatch = Rect::from_center_size(Pos2::new(x + SWATCH / 2.0, y), Vec2::splat(SWATCH));
        painter.rect_filled(swatch, 3.0, *fill);
        if let Some(stroke) = stroke {
            painter.rect_stroke(swatch, 3.0, Stroke::new(1.5, *stroke), StrokeKind::Inside);
        }
        let galley = galleys[i].clone();
        let pos = Pos2::new(x + SWATCH + 6.0, y - galley.size().y / 2.0);
        painter.galley(pos, galley, LABEL_TEXT);
        x += item_width(i) + 18.0;
    }
}

/// A display-only centre cell: yellow for the sung pitch class.
fn centre_cell(painter: &egui::Painter, rect: Rect, name: &str, stable: bool) {
    let (fill, text) = if stable {
        (STABLE, STABLE_TEXT)
    } else {
        (CENTRE_FILL, Color32::WHITE)
    };
    painter.rect_filled(rect, CORNER, fill);
    // The default fonts have no bold face: draw the label twice, offset.
    for dx in [0.0, 0.6] {
        painter.text(
            rect.center() + Vec2::new(dx, 0.0),
            Align2::CENTER_CENTER,
            name,
            FontId::proportional(14.0),
            text,
        );
    }
}

fn toggle_cell(
    painter: &egui::Painter,
    rect: Rect,
    name: &str,
    on: bool,
    sounding: bool,
    hovered: bool,
) {
    let (fill, text) = match (on, sounding) {
        (true, true) => (SOUNDING_FILL, ON_TEXT),
        (true, false) => (ON_FILL, ON_TEXT),
        (false, _) => (OFF_FILL, OFF_TEXT),
    };
    let stroke = if sounding {
        Stroke::new(2.0, STABLE)
    } else if hovered {
        Stroke::new(1.0, HOVER_STROKE)
    } else if on {
        Stroke::NONE
    } else {
        Stroke::new(1.0, OFF_STROKE)
    };
    painter.rect(rect, CORNER, fill, stroke, StrokeKind::Inside);
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        name,
        FontId::proportional(14.0),
        text,
    );
}

/// A monospace font at `size`, shrunk so `text` fits in `width` (a full row
/// lists 24 voices).
fn fitted_mono(painter: &egui::Painter, text: &str, size: f32, width: f32) -> FontId {
    let natural = painter
        .layout_no_wrap(text.to_owned(), FontId::monospace(size), Color32::WHITE)
        .size()
        .x;
    if natural <= width {
        FontId::monospace(size)
    } else {
        FontId::monospace((size * width / natural).max(8.0))
    }
}

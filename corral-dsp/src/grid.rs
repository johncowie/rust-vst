//! The harmony grid: for each pitch class of the stable note, which
//! intervals (−12…−1 and +1…+12 semitones) sound as voices.

use std::sync::atomic::{AtomicU32, Ordering};

use crate::note::{self, NAMES};

pub const PITCH_CLASSES: usize = 12;
/// Intervals in a row, so the most voices that can sound at once.
pub const MAX_VOICES: usize = 24;

/// Bit i of a row is `INTERVALS[i]`. Ascending, so reading the bits from
/// the lowest gives the intervals in ascending order.
#[rustfmt::skip]
pub const INTERVALS: [i8; MAX_VOICES] = [
    -12, -11, -10, -9, -8, -7, -6, -5, -4, -3, -2, -1,
    1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12,
];

/// Every row bit set.
const ALL: u32 = (1 << MAX_VOICES) - 1;

/// The bit for `interval`; `None` for 0 or beyond ±12.
pub fn bit(interval: i8) -> Option<u32> {
    let index = match interval {
        -12..=-1 => interval + 12,
        1..=12 => interval + 11,
        _ => return None,
    };
    Some(1 << index)
}

/// One row's "on" intervals as 24 bits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Row(u32);

impl Row {
    pub const EMPTY: Self = Self(0);

    /// Bits above 23 are dropped.
    pub fn from_bits(bits: u32) -> Self {
        Self(bits & ALL)
    }

    pub fn bits(self) -> u32 {
        self.0
    }

    pub fn contains(self, interval: i8) -> bool {
        bit(interval).is_some_and(|b| self.0 & b != 0)
    }

    /// `self` unchanged for an interval with no bit.
    pub fn with(self, interval: i8, on: bool) -> Self {
        match bit(interval) {
            Some(b) if on => Self(self.0 | b),
            Some(b) => Self(self.0 & !b),
            None => self,
        }
    }

    pub fn toggled(self, interval: i8) -> Self {
        bit(interval).map_or(self, |b| Self(self.0 ^ b))
    }

    pub fn len(self) -> usize {
        self.0.count_ones() as usize
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The on intervals in ascending order, without allocating.
    pub fn intervals(self) -> Intervals {
        Intervals(self.0)
    }
}

/// Iterates a row's bits lowest first.
#[derive(Clone)]
pub struct Intervals(u32);

impl Iterator for Intervals {
    type Item = i8;

    fn next(&mut self) -> Option<i8> {
        if self.0 == 0 {
            return None;
        }
        let index = self.0.trailing_zeros() as usize;
        self.0 &= self.0 - 1;
        Some(INTERVALS[index])
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let n = self.0.count_ones() as usize;
        (n, Some(n))
    }
}

impl ExactSizeIterator for Intervals {}

/// All twelve rows, C first. `Copy`, so `Settings` stays `Copy`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Grid {
    rows: [Row; PITCH_CLASSES],
}

impl Grid {
    pub const EMPTY: Self = Self {
        rows: [Row::EMPTY; PITCH_CLASSES],
    };

    pub fn row(&self, pitch_class: usize) -> Row {
        self.rows[pitch_class]
    }

    /// The row for a MIDI note's pitch class; any octave, negative too.
    pub fn row_for(&self, midi: i32) -> Row {
        self.rows[midi.rem_euclid(12) as usize]
    }

    pub fn set_row(&mut self, pitch_class: usize, row: Row) {
        self.rows[pitch_class] = row;
    }

    pub fn toggle(&mut self, pitch_class: usize, interval: i8) {
        self.rows[pitch_class] = self.rows[pitch_class].toggled(interval);
    }

    /// `{interval}` in every row: a fixed shift, for the shifter tests.
    pub fn uniform(interval: i8) -> Self {
        Self {
            rows: [Row::EMPTY.with(interval, true); PITCH_CLASSES],
        }
    }
}

/// The grid shared by the editor (which toggles cells) and the audio
/// thread (which copies it once per buffer). Lock-free: one atomic per
/// row. `Relaxed` is enough, because rows are independent and nothing
/// else is published with them.
#[derive(Default)]
pub struct SharedGrid {
    rows: [AtomicU32; PITCH_CLASSES],
}

impl SharedGrid {
    /// Empty.
    pub fn new() -> Self {
        Self::default()
    }

    /// Twelve relaxed loads; doesn't allocate.
    pub fn snapshot(&self) -> Grid {
        Grid {
            rows: std::array::from_fn(|pc| self.row(pc)),
        }
    }

    pub fn row(&self, pitch_class: usize) -> Row {
        Row::from_bits(self.rows[pitch_class].load(Ordering::Relaxed))
    }

    /// Flips the cell's bit; nothing for an interval with no bit.
    pub fn toggle(&self, pitch_class: usize, interval: i8) {
        if let Some(b) = bit(interval) {
            self.rows[pitch_class].fetch_xor(b, Ordering::Relaxed);
        }
    }
}

/// The note name in row `pitch_class`, column `interval` (0 = the
/// centre column).
pub fn cell_name(pitch_class: usize, interval: i8) -> &'static str {
    NAMES[(pitch_class as i32 + i32::from(interval)).rem_euclid(12) as usize]
}

/// The editor's readout: `"C4 → G3 · E4"`, or `"C4 → —"` for an empty row.
pub fn format_voices(stable: i32, row: Row) -> String {
    let mut text = format!("{} →", note::format_name(stable));
    if row.is_empty() {
        text.push_str(" —");
    }
    for (i, interval) in row.intervals().enumerate() {
        text.push_str(if i == 0 { " " } else { " · " });
        text.push_str(&note::format_name(stable + i32::from(interval)));
    }
    text
}

/// Case-insensitive "C", "c#", "Db", "Bb", "E#"…; `None` otherwise.
pub fn parse_pitch_class(text: &str) -> Option<usize> {
    let mut chars = text.chars();
    let natural: i32 = match chars.next()?.to_ascii_uppercase() {
        'C' => 0,
        'D' => 2,
        'E' => 4,
        'F' => 5,
        'G' => 7,
        'A' => 9,
        'B' => 11,
        _ => return None,
    };
    let accidental = match chars.next() {
        None => 0,
        Some('#') => 1,
        Some('b' | 'B') => -1,
        Some(_) => return None,
    };
    if chars.next().is_some() {
        return None;
    }
    Some((natural + accidental).rem_euclid(12) as usize)
}

/// One interval: ±1…12 with an optional `+`.
fn parse_interval(text: &str) -> Option<i8> {
    let digits = text.strip_prefix('+').unwrap_or(text);
    // `i8::from_str` would accept a second sign after a stripped `+`.
    if digits.starts_with('+') {
        return None;
    }
    let interval: i8 = digits.parse().ok()?;
    bit(interval).map(|_| interval)
}

/// Row specs such as `C:-5,+4` and `D:+3` (whitespace inside an item
/// separates specs too). Intervals are ±1…12 with an optional `+`. A
/// pitch class given twice, a repeated interval, 0 or beyond ±12 give
/// `None`, as does no spec at all.
pub fn parse_grid<'a>(specs: impl IntoIterator<Item = &'a str>) -> Option<Grid> {
    let mut grid = Grid::EMPTY;
    let mut seen = [false; PITCH_CLASSES];
    let mut any = false;
    for spec in specs.into_iter().flat_map(str::split_whitespace) {
        let (name, intervals) = spec.split_once(':')?;
        let pc = parse_pitch_class(name)?;
        if std::mem::replace(&mut seen[pc], true) {
            return None;
        }
        let mut row = Row::EMPTY;
        for text in intervals.split(',') {
            let interval = parse_interval(text)?;
            if row.contains(interval) {
                return None;
            }
            row = row.with(interval, true);
        }
        grid.set_row(pc, row);
        any = true;
    }
    any.then_some(grid)
}

/// `"C:-5,+4 D:+3"`, rows C…B, intervals ascending with signs; `"none"`
/// when empty. Round-trips through `parse_grid`.
pub fn format_grid(grid: &Grid) -> String {
    let specs: Vec<String> = (0..PITCH_CLASSES)
        .filter(|&pc| !grid.row(pc).is_empty())
        .map(|pc| {
            let intervals: Vec<String> =
                grid.row(pc).intervals().map(|i| format!("{i:+}")).collect();
            format!("{}:{}", NAMES[pc], intervals.join(","))
        })
        .collect();
    if specs.is_empty() {
        "none".to_owned()
    } else {
        specs.join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The reference model: each row's on intervals as a sorted `Vec`,
    /// kept without any of the bit logic.
    struct Model([Vec<i8>; PITCH_CLASSES]);

    impl Model {
        fn new() -> Self {
            Self(std::array::from_fn(|_| Vec::new()))
        }

        fn toggle(&mut self, pc: usize, interval: i8) {
            let row = &mut self.0[pc];
            match row.iter().position(|&i| i == interval) {
                Some(at) => {
                    row.remove(at);
                }
                None => row.push(interval),
            }
            row.sort();
        }

        fn check(&self, grid: &Grid, context: &str) {
            for pc in 0..PITCH_CLASSES {
                let got: Vec<i8> = grid.row(pc).intervals().collect();
                assert_eq!(got, self.0[pc], "row {pc} after {context}");
                assert_eq!(grid.row(pc).len(), self.0[pc].len(), "row {pc} len");
            }
        }
    }

    /// Cell `k` of the 288, row-major.
    fn cell(k: usize) -> (usize, i8) {
        (k / MAX_VOICES, INTERVALS[k % MAX_VOICES])
    }

    #[test]
    fn every_cell_toggles_on_and_off_alone() {
        for k in 0..PITCH_CLASSES * MAX_VOICES {
            let (pc, interval) = cell(k);
            let mut grid = Grid::EMPTY;
            let mut model = Model::new();
            grid.toggle(pc, interval);
            model.toggle(pc, interval);
            model.check(&grid, &format!("{pc} {interval:+} on"));
            grid.toggle(pc, interval);
            model.toggle(pc, interval);
            model.check(&grid, &format!("{pc} {interval:+} off"));
            assert_eq!(grid, Grid::EMPTY);
        }
    }

    #[test]
    fn every_cell_toggles_on_then_off_in_scrambled_orders() {
        const CELLS: usize = PITCH_CLASSES * MAX_VOICES;
        let mut grid = Grid::EMPTY;
        let mut model = Model::new();
        // 109 and 173 are coprime to 288, so each order visits every cell.
        for (order, factor) in [("on", 109), ("off", 173)] {
            for k in 0..CELLS {
                let (pc, interval) = cell(k * factor % CELLS);
                grid.toggle(pc, interval);
                model.toggle(pc, interval);
                model.check(&grid, &format!("{order} step {k}"));
            }
            let expected = if order == "on" { MAX_VOICES } else { 0 };
            for pc in 0..PITCH_CLASSES {
                assert_eq!(grid.row(pc).len(), expected, "row {pc} after {order}");
            }
        }
    }

    #[test]
    fn intervals_ascend_without_zero_and_bits_round_trip() {
        assert_eq!(INTERVALS.len(), 24);
        assert!(INTERVALS.windows(2).all(|w| w[0] < w[1]));
        assert!(!INTERVALS.contains(&0));
        for (i, &interval) in INTERVALS.iter().enumerate() {
            assert_eq!(bit(interval), Some(1 << i), "{interval}");
            let row = Row::from_bits(1 << i);
            assert_eq!(row.intervals().collect::<Vec<_>>(), [interval]);
        }
        for interval in [0, 13, -13, i8::MIN, i8::MAX] {
            assert_eq!(bit(interval), None, "{interval}");
        }
    }

    #[test]
    fn row_for_ignores_the_octave() {
        let mut grid = Grid::EMPTY;
        grid.set_row(0, Row::EMPTY.with(4, true));
        grid.set_row(11, Row::EMPTY.with(-5, true));
        for midi in [48, 60, 72, -12] {
            assert_eq!(grid.row_for(midi), grid.row(0), "{midi}");
        }
        assert_eq!(grid.row_for(-1), grid.row(11));
    }

    #[test]
    fn rows_ignore_intervals_with_no_bit() {
        assert_eq!(Row::from_bits(u32::MAX).len(), 24);
        let row = Row::EMPTY.with(4, true);
        for interval in [0, 13, -13] {
            assert_eq!(row.with(interval, true), row);
            assert_eq!(row.with(interval, false), row);
            assert_eq!(row.toggled(interval), row);
            assert!(!row.contains(interval));
        }
        assert!(row.contains(4));
        assert!(row.with(4, false).is_empty());
        assert_eq!(
            Grid::uniform(-7).row(5).intervals().collect::<Vec<_>>(),
            [-7]
        );
    }

    #[test]
    fn shared_grid_sees_toggles_from_another_thread() {
        let shared = std::sync::Arc::new(SharedGrid::new());
        let writer = std::sync::Arc::clone(&shared);
        std::thread::spawn(move || {
            writer.toggle(0, -5);
            writer.toggle(0, 4);
            writer.toggle(2, 3);
            writer.toggle(11, 12);
            writer.toggle(11, 12);
            writer.toggle(11, 0);
        })
        .join()
        .unwrap();
        let grid = shared.snapshot();
        assert_eq!(grid.row(0).intervals().collect::<Vec<_>>(), [-5, 4]);
        assert_eq!(grid.row(2).intervals().collect::<Vec<_>>(), [3]);
        assert!(grid.row(11).is_empty());
        assert_eq!(shared.row(0), grid.row(0));
    }

    #[test]
    fn cells_are_named_from_the_row_and_interval() {
        assert_eq!(cell_name(0, -5), "G");
        assert_eq!(cell_name(11, 1), "C");
        assert_eq!(cell_name(0, -12), "C");
        assert_eq!(cell_name(0, 12), "C");
        for (pc, name) in NAMES.iter().enumerate() {
            assert_eq!(cell_name(pc, 0), *name);
        }
    }

    #[test]
    fn readout_lists_the_voice_notes() {
        let row = Row::EMPTY.with(-5, true).with(4, true);
        assert_eq!(format_voices(60, row), "C4 → G3 · E4");
        assert_eq!(format_voices(60, Row::EMPTY.with(4, true)), "C4 → E4");
        assert_eq!(format_voices(60, Row::EMPTY), "C4 → —");
    }

    #[test]
    fn pitch_classes_round_trip() {
        for (pc, name) in NAMES.iter().enumerate() {
            assert_eq!(parse_pitch_class(name), Some(pc));
        }
    }

    #[test]
    fn pitch_classes_accept_flats_and_lower_case() {
        assert_eq!(parse_pitch_class("c"), Some(0));
        assert_eq!(parse_pitch_class("c#"), Some(1));
        assert_eq!(parse_pitch_class("Db"), Some(1));
        assert_eq!(parse_pitch_class("db"), Some(1));
        assert_eq!(parse_pitch_class("Bb"), Some(10));
        assert_eq!(parse_pitch_class("a#"), Some(10));
        assert_eq!(parse_pitch_class("E#"), Some(5));
        assert_eq!(parse_pitch_class("Cb"), Some(11));
    }

    #[test]
    fn bad_pitch_classes_are_rejected() {
        for text in ["H", "", "C##", "Cb5", "#", "C ", "major"] {
            assert_eq!(parse_pitch_class(text), None, "{text:?}");
        }
    }

    #[test]
    fn grids_parse_from_separate_or_joined_specs() {
        let separate = parse_grid(["C:-5,+4", "D:+3"]).unwrap();
        let joined = parse_grid(["C:-5,+4 D:+3"]).unwrap();
        assert_eq!(separate, joined);
        assert_eq!(separate.row(0).intervals().collect::<Vec<_>>(), [-5, 4]);
        assert_eq!(separate.row(2).intervals().collect::<Vec<_>>(), [3]);
        for pc in [1, 3, 4, 5, 6, 7, 8, 9, 10, 11] {
            assert!(separate.row(pc).is_empty(), "row {pc}");
        }
        let flat = parse_grid(["db:3"]).unwrap();
        assert_eq!(flat.row(1).intervals().collect::<Vec<_>>(), [3]);
        let full = parse_grid(["B:+12,-12"]).unwrap();
        assert_eq!(full.row(11).intervals().collect::<Vec<_>>(), [-12, 12]);
    }

    #[test]
    fn bad_grids_are_rejected() {
        for specs in [
            &["C"][..],
            &["C:"],
            &["C:0"],
            &["C:+13"],
            &["C:-13"],
            &["H:+1"],
            &["C:+4,+4"],
            &["C:+4 C:+3"],
            &["C:+4", "C:+3"],
            &["C:four"],
            &["C:++4"],
            &["C:+4,"],
            &[],
            &[""],
        ] {
            assert_eq!(parse_grid(specs.iter().copied()), None, "{specs:?}");
        }
    }

    #[test]
    fn grids_format_and_round_trip() {
        assert_eq!(format_grid(&Grid::EMPTY), "none");
        let grid = parse_grid(["D:+3 C:+4,-5"]).unwrap();
        assert_eq!(format_grid(&grid), "C:-5,+4 D:+3");
        let mut full = Grid::EMPTY;
        full.set_row(11, Row::from_bits(u32::MAX));
        for grid in [grid, Grid::uniform(-12), Grid::uniform(12), full] {
            let text = format_grid(&grid);
            assert_eq!(parse_grid(text.split(' ')), Some(grid), "{text}");
        }
    }
}

//! User settings, kept in `settings.ini` in the addon directory as `key = value` lines.

use crate::{files::write_atomic, format::fmt_float};
use std::{fs, io, ops::RangeInclusive, path::Path};

pub const RAISE_RANGE: RangeInclusive<f32> = 0.0..=5.0;
pub const SPACING_RANGE: RangeInclusive<f32> = 0.25..=5.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// Refuse a marker when one of the same category on the same map is close by.
    pub skip_near_duplicates: bool,
    /// Add [`Settings::raise_by`] to the `ypos` of new markers.
    pub raise_markers: bool,
    /// Metres.
    pub raise_by: f32,
    /// Distance in metres between recorded trail points.
    pub trail_spacing: f32,
    /// Trail points are only placed by hand, never recorded as the character moves.
    pub trail_manual: bool,
    /// Draw the pack's markers and trails in the world.
    pub preview: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            skip_near_duplicates: true,
            raise_markers: false,
            // Head height, the same as the default heightOffset.
            raise_by: 1.5,
            // TacO records every ~0.76 m (docs/01 §8). 1 m is as smooth in practice.
            trail_spacing: 1.0,
            trail_manual: false,
            preview: true,
        }
    }
}

impl Settings {
    /// Reads `path`. Missing or unreadable file, unknown keys and bad values give defaults.
    pub fn load(path: &Path) -> Self {
        fs::read_to_string(path)
            .map(|text| Self::parse(&text))
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        write_atomic(path, self.to_text())
    }

    pub fn parse(text: &str) -> Self {
        let mut settings = Self::default();
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim();
            match key.trim() {
                "skip_near_duplicates" => set_bool(&mut settings.skip_near_duplicates, value),
                "raise_markers" => set_bool(&mut settings.raise_markers, value),
                "raise_by" => set_float(&mut settings.raise_by, value, RAISE_RANGE),
                "trail_spacing" => set_float(&mut settings.trail_spacing, value, SPACING_RANGE),
                "trail_manual" => set_bool(&mut settings.trail_manual, value),
                "preview" => set_bool(&mut settings.preview, value),
                _ => {}
            }
        }
        settings
    }

    pub fn to_text(&self) -> String {
        let bool = |b: bool| if b { "1" } else { "0" };
        format!(
            "# Pathing Editor settings. Changed from the addon window.\n\
             skip_near_duplicates = {}\n\
             raise_markers = {}\n\
             raise_by = {}\n\
             trail_spacing = {}\n\
             trail_manual = {}\n\
             preview = {}\n",
            bool(self.skip_near_duplicates),
            bool(self.raise_markers),
            fmt_float(self.raise_by, 3),
            fmt_float(self.trail_spacing, 3),
            bool(self.trail_manual),
            bool(self.preview),
        )
    }

    /// How far new markers are raised, in metres.
    pub fn marker_raise(&self) -> f32 {
        if self.raise_markers {
            clamp(self.raise_by, RAISE_RANGE)
        } else {
            0.0
        }
    }

    pub fn spacing(&self) -> f32 {
        clamp(self.trail_spacing, SPACING_RANGE)
    }
}

fn set_bool(field: &mut bool, value: &str) {
    match value {
        "1" | "true" => *field = true,
        "0" | "false" => *field = false,
        _ => {}
    }
}

fn set_float(field: &mut f32, value: &str, range: RangeInclusive<f32>) {
    if let Ok(v) = value.parse::<f32>()
        && v.is_finite()
    {
        *field = clamp(v, range);
    }
}

fn clamp(value: f32, range: RangeInclusive<f32>) -> f32 {
    if value.is_finite() {
        value.clamp(*range.start(), *range.end())
    } else {
        *range.start()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let settings = Settings {
            skip_near_duplicates: false,
            raise_markers: true,
            raise_by: 1.25,
            trail_spacing: 2.0,
            trail_manual: true,
            preview: false,
        };
        assert_eq!(Settings::parse(&settings.to_text()), settings);
    }

    #[test]
    fn bad_input_gives_defaults_or_clamps() {
        let parsed = Settings::parse(
            "garbage\nraise_markers = maybe\nraise_by = 99\ntrail_spacing = NaN\nunknown = 1\n",
        );
        assert_eq!(
            parsed,
            Settings {
                raise_by: 5.0,
                ..Settings::default()
            }
        );
    }

    #[test]
    fn raise_only_when_enabled() {
        let mut settings = Settings::default();
        assert_eq!(settings.marker_raise(), 0.0);
        settings.raise_markers = true;
        assert_eq!(settings.marker_raise(), 1.5);
        settings.raise_by = -3.0;
        assert_eq!(settings.marker_raise(), 0.0, "clamped");
    }
}

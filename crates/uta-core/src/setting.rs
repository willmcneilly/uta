//! What a setting is, for anything that shows or explains one: the synth's,
//! the drums', and each effect's to come. See RFC-008, "In the project" and
//! "Factory sounds", and RFC-007, "Patterns for every effect after this
//! one", point 2.
//!
//! Each setting is described once, here in the core next to its range, so
//! the panel, the docs and a sound designer all read the same words, and the
//! UI never names a range, a default or a label itself.

use serde::Serialize;

/// One setting, described: what it's called, how it sounds, and its range
/// or its choices.
///
/// Serialises flat, as `{"label":"Cutoff","sounds":"…","kind":"number",
/// "unit":"hz","scale":"log","limits":[20.0,20000.0],"default":20000.0}`,
/// or `{"label":"Waveform","sounds":"…","kind":"choice","options":[…],
/// "default":"saw"}`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Description {
    /// What the panel calls it: "Cutoff", "Tune".
    pub label: &'static str,
    /// How it sounds, not just what it is: what turning it does to the
    /// sound, and what its ends are good for.
    pub sounds: &'static str,
    #[serde(flatten)]
    pub kind: Kind,
}

/// A setting's values: a number in a range, one of a few choices, or on or
/// off.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Kind {
    Number {
        /// The unit the value, the limits and the default are in.
        unit: Unit,
        /// How a control moves through the range.
        scale: Scale,
        /// The inclusive `(min, max)`.
        limits: (f32, f32),
        /// A new track's value, which a reset sets it to.
        default: f32,
        /// The step between values, for a setting that only takes some:
        /// 1 for an octave. Left out for one that takes any value in
        /// `limits`.
        #[serde(skip_serializing_if = "Option::is_none")]
        step: Option<f32>,
    },
    Choice {
        /// Every choice, in the panel's order.
        options: &'static [Choice],
        /// A new track's choice, as its command carries it: `"saw"`.
        default: &'static str,
    },
    /// A switch: on or off.
    Toggle {
        /// Whether it's on for a new track.
        default: bool,
    },
}

/// One choice of a setting that has them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Choice {
    /// The choice as its command carries it: `"saw"`, `"808"`.
    pub value: &'static str,
    /// What the panel calls it: "Saw", "808".
    pub label: &'static str,
}

/// The unit a setting's value is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    Hz,
    Seconds,
    Db,
    /// From 0 to 1: how much of something, such as the snare's Snappy or
    /// the synth's Sustain. It reads as a percentage.
    Fraction,
    /// From 0 to 1, but not a share of anything: the filter's resonance. It
    /// reads as the number itself.
    Amount,
    /// A pitch offset: 12 to the octave.
    Semitones,
    /// A distance in octaves: whole ones for an oscillator's Octave, where
    /// the description has a step of 1, and any for the filter envelope's
    /// amount, which moves the cutoff.
    Octaves,
}

/// How a control moves through a setting's range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Scale {
    /// Each step is the same amount: levels and amounts.
    Linear,
    /// Each step is the same ratio, the way you hear frequencies and times.
    Log,
    /// Fine near the middle of the range and coarse towards its ends, for an
    /// offset either side of zero, such as oscillator 2's Pitch: a few cents
    /// is a slow beat, and a few semitones is another note. A control's
    /// distance from its centre is cubed, so half way out is an eighth of
    /// the way to the end. The limits are either side of zero by the same
    /// amount.
    Cubic,
}

impl Description {
    /// A number in `limits`, in `unit`, moving on `scale`.
    pub(crate) const fn number(
        label: &'static str,
        sounds: &'static str,
        unit: Unit,
        scale: Scale,
        limits: (f32, f32),
        default: f32,
    ) -> Self {
        Self {
            label,
            sounds,
            kind: Kind::Number {
                unit,
                scale,
                limits,
                default,
                step: None,
            },
        }
    }

    /// A whole number in `limits`, in `unit`, such as an octave.
    pub(crate) const fn whole(
        label: &'static str,
        sounds: &'static str,
        unit: Unit,
        limits: (f32, f32),
        default: f32,
    ) -> Self {
        Self {
            label,
            sounds,
            kind: Kind::Number {
                unit,
                scale: Scale::Linear,
                limits,
                default,
                step: Some(1.0),
            },
        }
    }

    /// One of `options`.
    pub(crate) const fn choice(
        label: &'static str,
        sounds: &'static str,
        options: &'static [Choice],
        default: &'static str,
    ) -> Self {
        Self {
            label,
            sounds,
            kind: Kind::Choice { options, default },
        }
    }

    /// On or off.
    pub(crate) const fn toggle(label: &'static str, sounds: &'static str, default: bool) -> Self {
        Self {
            label,
            sounds,
            kind: Kind::Toggle { default },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_description_serialises_flat() {
        let number = Description::number(
            "Cutoff",
            "How bright.",
            Unit::Hz,
            Scale::Log,
            (20.0, 20_000.0),
            20_000.0,
        );
        assert_eq!(
            serde_json::to_value(number).unwrap(),
            serde_json::json!({
                "label": "Cutoff",
                "sounds": "How bright.",
                "kind": "number",
                "unit": "hz",
                "scale": "log",
                "limits": [20.0, 20_000.0],
                "default": 20_000.0,
            })
        );
        const OPTIONS: &[Choice] = &[Choice {
            value: "saw",
            label: "Saw",
        }];
        let choice = Description::choice("Waveform", "Its shape.", OPTIONS, "saw");
        assert_eq!(
            serde_json::to_value(choice).unwrap(),
            serde_json::json!({
                "label": "Waveform",
                "sounds": "Its shape.",
                "kind": "choice",
                "options": [{"value": "saw", "label": "Saw"}],
                "default": "saw",
            })
        );
        let whole = Description::whole("Octave", "How high.", Unit::Octaves, (-2.0, 2.0), 0.0);
        assert_eq!(
            serde_json::to_value(whole).unwrap(),
            serde_json::json!({
                "label": "Octave",
                "sounds": "How high.",
                "kind": "number",
                "unit": "octaves",
                "scale": "linear",
                "limits": [-2.0, 2.0],
                "default": 0.0,
                "step": 1.0,
            })
        );
        let toggle = Description::toggle("Retrigger", "Whether it restarts.", true);
        assert_eq!(
            serde_json::to_value(toggle).unwrap(),
            serde_json::json!({
                "label": "Retrigger",
                "sounds": "Whether it restarts.",
                "kind": "toggle",
                "default": true,
            })
        );
    }
}

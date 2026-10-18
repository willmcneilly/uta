//! The song file as TOML reads it, before it's compiled. Everything a
//! mistake can be in keeps its place in the file (a [`Spanned`]), so the
//! error can say which line it's on.

use std::fmt;
use std::marker::PhantomData;

use serde::Deserialize;
use serde::de::value::MapAccessDeserializer;
use serde::de::{MapAccess, SeqAccess, Visitor};
use toml::{Spanned, Value};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SongFile {
    /// The song's name, which its project's ID is made from.
    pub name: Spanned<String>,
    /// In beats (quarter notes) a minute.
    pub tempo: Spanned<f32>,
    /// The sections in the order they play, by name. Each section once, in
    /// the order they're written, if it's left out.
    pub arrangement: Option<Spanned<Vec<Spanned<String>>>>,
    /// Whether the loop is on, round the whole arrangement. On if it's left
    /// out.
    #[serde(rename = "loop")]
    pub looped: Option<Spanned<bool>>,
    pub master: Option<MasterFile>,
    #[serde(default, rename = "track")]
    pub tracks: Vec<Spanned<TrackFile>>,
    #[serde(default, rename = "section")]
    pub sections: Vec<Spanned<SectionFile>>,
    /// Raw commands, for anything the song file doesn't cover.
    #[serde(default, rename = "command")]
    pub commands: Vec<Spanned<toml::Table>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MasterFile {
    pub volume_db: Option<Spanned<f32>>,
    #[serde(default)]
    pub effects: Vec<Spanned<Entries<Value>>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TrackFile {
    pub name: Spanned<String>,
    pub kind: Spanned<Kind>,
    /// A factory sound, by its key or its name. Synth tracks only.
    pub sound: Option<Spanned<String>>,
    /// Settings by name: a synth's, or for drums, each sound's in its own
    /// table.
    pub settings: Option<Entries<Value>>,
    pub mixer: Option<Spanned<MixerFile>>,
    #[serde(default)]
    pub effects: Vec<Spanned<Entries<Value>>>,
    /// What the track plays in each section, by the section's name.
    #[serde(default)]
    pub pattern: Entries<PatternFile>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Kind {
    Synth,
    Drums,
}

/// A track's mixer strip. What's left out keeps a new track's value.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MixerFile {
    pub volume_db: Option<f32>,
    pub pan: Option<f32>,
    pub mute: Option<bool>,
    pub solo: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SectionFile {
    pub name: Spanned<String>,
    pub bars: Spanned<u32>,
}

/// What a track plays in a section: one line of steps, a table (a drum
/// pattern, or a line with its own step, length or velocity), or several
/// lines played together.
#[derive(Debug)]
pub(crate) enum PatternFile {
    Steps(String),
    Table(Entries<Value>),
    Lines(Vec<Spanned<PatternFile>>),
}

/// A table's entries in the order they're written, each key and value with
/// its place in the file.
#[derive(Debug)]
pub(crate) struct Entries<V>(pub Vec<(Spanned<String>, Spanned<V>)>);

impl<V> Default for Entries<V> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<'de, V: Deserialize<'de>> Deserialize<'de> for Entries<V> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct EntriesVisitor<V>(PhantomData<V>);

        impl<'de, V: Deserialize<'de>> Visitor<'de> for EntriesVisitor<V> {
            type Value = Entries<V>;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a table")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut entries = Vec::new();
                while let Some(key) = map.next_key()? {
                    entries.push((key, map.next_value()?));
                }
                Ok(Entries(entries))
            }
        }

        deserializer.deserialize_map(EntriesVisitor(PhantomData))
    }
}

impl<'de> Deserialize<'de> for PatternFile {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct PatternVisitor;

        impl<'de> Visitor<'de> for PatternVisitor {
            type Value = PatternFile;

            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a string of steps, a table, or a list of them")
            }

            fn visit_str<E: serde::de::Error>(self, steps: &str) -> Result<Self::Value, E> {
                Ok(PatternFile::Steps(steps.to_owned()))
            }

            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
                Entries::deserialize(MapAccessDeserializer::new(map)).map(PatternFile::Table)
            }

            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut lines = Vec::new();
                while let Some(line) = seq.next_element()? {
                    lines.push(line);
                }
                Ok(PatternFile::Lines(lines))
            }
        }

        deserializer.deserialize_any(PatternVisitor)
    }
}

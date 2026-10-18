//! Songs written in musical terms, in a TOML song file, and compiled to a
//! [`CommandList`]. See RFC-009, "The demo song". The format is documented
//! in `examples/demos/README.md`.
//!
//! A song file has a tempo, tracks (each a synth or the drums, with a
//! sound, settings by name, a mixer strip and effects), what each track
//! plays in each section, the sections with their lengths in bars, and an
//! arrangement of them. Anything else is written as raw commands inside the
//! file.
//!
//! [`compile`] turns it into the commands that build it, and applies each
//! to a project as it goes, so a song always builds with today's command
//! format, and a value out of range is an error on its line, worded as the
//! command's. Every ID is worked out from the song's name, so compiling
//! the same file always gives the same commands.

mod file;
mod steps;

use std::collections::HashMap;
use std::ops::Range;

use serde::de::DeserializeOwned;
use toml::{Spanned, Value};
use uta_core::time::{NoteDivision, Ticks};
use uta_core::{
    COMMAND_FORMAT, Chain, Clip, ClipId, Command, CommandList, DelayParam, DrumParam, DrumSound,
    Effect, EffectId, EffectKind, FactorySound, KIT, KitSettings, MixerStrip, Note, NoteId,
    PlacedClip, PlacedEffect, PlacedTrack, Project, ProjectId, Source, SynthParam, SynthSettings,
    Track, TrackId,
};
use uuid::Uuid;

use file::{Entries, Kind, PatternFile, SongFile, TrackFile};
use steps::Line;

/// A mistake in a song file, and the line it's on, counting from 1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SongError {
    pub line: usize,
    pub message: String,
}

impl std::fmt::Display for SongError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "line {}: {}", self.line, self.message)
    }
}

impl std::error::Error for SongError {}

/// The velocity of a note or hit that doesn't give one, where its line
/// doesn't either.
pub const DEFAULT_VELOCITY: u8 = 100;

/// A line's step where it doesn't give one: a sixteenth.
const DEFAULT_STEP: &str = "1/16";

/// Compiles a song file to the commands that build it.
pub fn compile(text: &str) -> Result<CommandList, SongError> {
    let song: SongFile = toml::from_str(text).map_err(|error| SongError {
        line: error.span().map_or(1, |span| line_of(text, span.start)),
        message: error.message().trim_end().to_owned(),
    })?;
    let id = ProjectId::from_uuid(Uuid::new_v5(&namespace(), song.name.get_ref().as_bytes()));
    let mut compiler = Compiler {
        text,
        id,
        project: Project::with_id(id),
        commands: Vec::new(),
        tracks: Vec::new(),
        effects: HashMap::new(),
    };
    compiler.song(&song)?;
    Ok(CommandList {
        project: id,
        commands: compiler.commands,
    })
}

/// The namespace songs' project IDs are made in, from their names.
fn namespace() -> Uuid {
    Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        b"https://github.com/willmcneilly/uta/songs",
    )
}

/// The line `offset` is on, counting from 1.
fn line_of(text: &str, offset: usize) -> usize {
    text[..offset.min(text.len())].matches('\n').count() + 1
}

struct Compiler<'a> {
    text: &'a str,
    id: ProjectId,
    /// The project so far: each command is applied to it as it's made, so
    /// a mistake is found on the line that made it.
    project: Project,
    commands: Vec<Command>,
    /// Each track the song adds, by name, with its clips in order, for raw
    /// commands to name.
    tracks: Vec<(String, TrackId, Vec<ClipId>)>,
    /// Each chain's effects in order, for raw commands to name.
    effects: HashMap<Chain, Vec<EffectId>>,
}

/// One section of the arrangement: where it starts and how long it is.
struct Placed {
    section: usize,
    start: Ticks,
    length: Ticks,
}

/// A note in a section, before it has an ID.
struct SectionNote {
    start: Ticks,
    length: Ticks,
    pitch: u8,
    velocity: u8,
}

impl Compiler<'_> {
    fn song(&mut self, song: &SongFile) -> Result<(), SongError> {
        self.push(
            Command::SetTempo {
                bpm: *song.tempo.get_ref(),
            },
            song.tempo.span(),
        )?;
        if let Some(master) = &song.master {
            if let Some(volume) = &master.volume_db {
                self.push(
                    Command::SetMasterVolume {
                        volume_db: *volume.get_ref(),
                    },
                    volume.span(),
                )?;
            }
            self.effects(Chain::Master, &master.effects, "song master")?;
        }
        let arrangement = self.arrangement(song)?;
        if song.tracks.is_empty() {
            return Err(self.error(0..0, "a song needs at least one [[track]]"));
        }
        let defaults: Vec<TrackId> = self.project.tracks().iter().map(Track::id).collect();
        for track in &song.tracks {
            self.track(track, song, &arrangement)?;
        }
        self.push(Command::RemoveTracks { tracks: defaults }, 0..0)?;
        let end = arrangement
            .last()
            .map_or(0, |placed| placed.start + placed.length);
        let bar = self.bar();
        let bars = u32::try_from(end / bar).unwrap_or(u32::MAX);
        self.push(Command::SetLoop { start_bar: 0, bars }, 0..0)?;
        if let Some(looped) = &song.looped
            && !looped.get_ref()
        {
            self.push(Command::SetLoopEnabled { enabled: false }, looped.span())?;
        }
        for raw in &song.commands {
            self.raw(raw)?;
        }
        Ok(())
    }

    fn bar(&self) -> Ticks {
        self.project.transport().time_signature().ticks_per_bar()
    }

    /// The arrangement's sections in order, each with where it starts.
    fn arrangement(&self, song: &SongFile) -> Result<Vec<Placed>, SongError> {
        if song.sections.is_empty() {
            return Err(self.error(0..0, "a song needs at least one [[section]]"));
        }
        for (index, section) in song.sections.iter().enumerate() {
            let section = section.get_ref();
            if *section.bars.get_ref() == 0 {
                return Err(self.error(section.bars.span(), "a section is at least 1 bar"));
            }
            let name = section.name.get_ref();
            if song.sections[..index]
                .iter()
                .any(|other| other.get_ref().name.get_ref() == name)
            {
                return Err(self.error(
                    section.name.span(),
                    format!("there's already a section called `{name}`"),
                ));
            }
        }
        let order: Vec<usize> = match &song.arrangement {
            None => (0..song.sections.len()).collect(),
            Some(names) if names.get_ref().is_empty() => {
                return Err(self.error(names.span(), "an arrangement names at least one section"));
            }
            Some(names) => names
                .get_ref()
                .iter()
                .map(|name| self.section(song, name))
                .collect::<Result<_, _>>()?,
        };
        let bar = self.bar();
        let mut start = 0;
        Ok(order
            .into_iter()
            .map(|section| {
                let length = Ticks::from(*song.sections[section].get_ref().bars.get_ref()) * bar;
                let placed = Placed {
                    section,
                    start,
                    length,
                };
                start += length;
                placed
            })
            .collect())
    }

    /// The index of the section `name` names.
    fn section(&self, song: &SongFile, name: &Spanned<String>) -> Result<usize, SongError> {
        song.sections
            .iter()
            .position(|section| section.get_ref().name.get_ref() == name.get_ref())
            .ok_or_else(|| {
                self.error(
                    name.span(),
                    format!("there's no section called `{}`", name.get_ref()),
                )
            })
    }

    fn track(
        &mut self,
        track: &Spanned<TrackFile>,
        song: &SongFile,
        arrangement: &[Placed],
    ) -> Result<(), SongError> {
        let span = track.span();
        let track = track.get_ref();
        let name = track.name.get_ref();
        if self.tracks.iter().any(|(other, ..)| other == name) {
            return Err(self.error(
                track.name.span(),
                format!("there's already a track called `{name}`"),
            ));
        }
        let prefix = format!("song track {name}");
        let id = TrackId::from_uuid(self.derived(&prefix));
        let kind = *track.kind.get_ref();
        let source = match kind {
            Kind::Synth => Source::Synth(SynthSettings::default()),
            Kind::Drums => Source::Drums(KitSettings::default()),
        };
        let index = self.project.tracks().len();
        self.push(
            Command::AddTracks {
                tracks: vec![PlacedTrack {
                    index,
                    track: Track::new(id, name.clone(), source),
                }],
            },
            span.clone(),
        )?;
        self.tracks.push((name.clone(), id, Vec::new()));
        self.sound(id, kind, track)?;
        if let Some(mixer) = &track.mixer {
            let file = mixer.get_ref();
            let default = MixerStrip::default();
            self.push(
                Command::SetTrackMixer {
                    track: id,
                    mixer: MixerStrip {
                        volume_db: file.volume_db.unwrap_or(default.volume_db),
                        pan: file.pan.unwrap_or(default.pan),
                        mute: file.mute.unwrap_or(default.mute),
                        solo: file.solo.unwrap_or(default.solo),
                    },
                },
                mixer.span(),
            )?;
        }
        self.effects(Chain::Track(id), &track.effects, &prefix)?;

        // What it plays in each section, then a clip for each time a
        // section it plays in comes round.
        let mut patterns: HashMap<usize, Vec<SectionNote>> = HashMap::new();
        for (section_name, pattern) in &track.pattern.0 {
            let section = self.section(song, section_name)?;
            let bars = *song.sections[section].get_ref().bars.get_ref();
            let length = Ticks::from(bars) * self.bar();
            patterns.insert(section, self.pattern(kind, pattern, length, bars)?);
        }
        let mut clips = Vec::new();
        for (position, placed) in arrangement.iter().enumerate() {
            let Some(notes) = patterns.get(&placed.section) else {
                continue;
            };
            let clip = format!("{prefix} clip {}", position + 1);
            let notes = notes.iter().enumerate().map(|(n, note)| Note {
                id: NoteId::from_uuid(self.derived(&format!("{clip} note {}", n + 1))),
                pitch: note.pitch,
                velocity: note.velocity,
                start: note.start,
                length: note.length,
            });
            clips.push(PlacedClip {
                track: id,
                clip: Clip::new(
                    ClipId::from_uuid(self.derived(&clip)),
                    placed.start,
                    placed.length,
                )
                .with_notes(notes),
            });
        }
        if !clips.is_empty() {
            let ids = clips.iter().map(|placed| placed.clip.id());
            self.tracks.last_mut().expect("just added").2 = ids.collect();
            self.push(Command::AddClips { clips }, span)?;
        }
        Ok(())
    }

    /// A track's factory sound and its settings by name.
    fn sound(&mut self, id: TrackId, kind: Kind, track: &TrackFile) -> Result<(), SongError> {
        if let Some(sound) = &track.sound {
            if kind == Kind::Drums {
                return Err(self.error(
                    sound.span(),
                    "a drum track plays the kit, and its sounds are set in `settings`",
                ));
            }
            let name = sound.get_ref();
            let factory = FactorySound::all()
                .into_iter()
                .find(|factory| {
                    factory.key.eq_ignore_ascii_case(name)
                        || factory.name.eq_ignore_ascii_case(name)
                })
                .ok_or_else(|| {
                    let keys: Vec<_> = FactorySound::all().iter().map(|sound| sound.key).collect();
                    self.error(
                        sound.span(),
                        format!(
                            "`{name}` isn't a factory sound: they're {}",
                            keys.join(", ")
                        ),
                    )
                })?;
            self.push(
                Command::SetSynthSettings {
                    track: id,
                    settings: factory.settings,
                },
                sound.span(),
            )?;
        }
        let Some(settings) = &track.settings else {
            return Ok(());
        };
        for (key, value) in &settings.0 {
            match kind {
                Kind::Synth => {
                    let param: SynthParam = param("synth", key.get_ref(), value.get_ref())
                        .map_err(|message| self.error(key.span(), message))?;
                    self.push(Command::SetSynthParam { track: id, param }, value.span())?;
                }
                Kind::Drums => {
                    let sound = drum_sound(key.get_ref())
                        .map_err(|message| self.error(key.span(), message))?;
                    let Value::Table(table) = value.get_ref() else {
                        return Err(self.error(
                            value.span(),
                            format!(
                                "a drum's settings are a table: {} = {{ ... }}",
                                key.get_ref()
                            ),
                        ));
                    };
                    // The kick's model first: the others' limits are its.
                    let mut entries: Vec<_> = table.iter().collect();
                    entries.sort_by_key(|(name, _)| name.as_str() != "model");
                    for (name, setting) in entries {
                        let param: DrumParam = param("drum", name, setting)
                            .map_err(|message| self.error(value.span(), message))?;
                        self.push(
                            Command::SetDrumParam {
                                track: id,
                                sound,
                                param,
                            },
                            value.span(),
                        )?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Adds a chain's effects, each with its settings by name, its Mix and
    /// whether it's on.
    fn effects(
        &mut self,
        chain: Chain,
        effects: &[Spanned<Entries<Value>>],
        prefix: &str,
    ) -> Result<(), SongError> {
        for (n, effect) in effects.iter().enumerate() {
            let span = effect.span();
            let mut kind = None;
            let mut mix = None;
            let mut enabled = None;
            for (key, value) in &effect.get_ref().0 {
                let wrong = |what: &str| {
                    self.error(
                        value.span(),
                        format!("an effect's `{}` is {what}", key.get_ref()),
                    )
                };
                match key.get_ref().as_str() {
                    "mix" => mix = Some(number(value.get_ref()).ok_or_else(|| wrong("a number"))?),
                    "enabled" => {
                        enabled = Some(
                            value
                                .get_ref()
                                .as_bool()
                                .ok_or_else(|| wrong("true or false"))?,
                        )
                    }
                    name => {
                        let found = EffectKind::ALL
                            .into_iter()
                            .find(|kind| kind.name().eq_ignore_ascii_case(name))
                            .ok_or_else(|| {
                                let kinds: Vec<_> = EffectKind::ALL
                                    .iter()
                                    .map(|kind| kind.name().to_lowercase())
                                    .collect();
                                self.error(
                                    key.span(),
                                    format!(
                                        "`{name}` isn't an effect: they're {}, and an entry can also have mix and enabled",
                                        kinds.join(", ")
                                    ),
                                )
                            })?;
                        if kind.is_some() {
                            return Err(self.error(key.span(), "one effect to an entry"));
                        }
                        kind = Some((found, value));
                    }
                }
            }
            let Some((kind, settings)) = kind else {
                return Err(self.error(
                    span,
                    "an effect entry names its effect: { delay = { feedback = 0.4 } }",
                ));
            };
            let id = EffectId::from_uuid(self.derived(&format!("{prefix} effect {}", n + 1)));
            let index = self.project.chain(chain).map_or(0, <[Effect]>::len);
            self.push(
                Command::AddEffects {
                    effects: vec![PlacedEffect {
                        chain,
                        index,
                        effect: Effect::default_of(id, kind),
                    }],
                },
                span.clone(),
            )?;
            self.effects.entry(chain).or_default().push(id);
            let Value::Table(table) = settings.get_ref() else {
                return Err(self.error(
                    settings.span(),
                    "an effect's settings are a table: { feedback = 0.4 }",
                ));
            };
            for (name, setting) in table {
                let command = match kind {
                    EffectKind::Delay => Command::SetDelayParam {
                        effect: id,
                        param: param::<DelayParam>("Delay", name, setting)
                            .map_err(|message| self.error(settings.span(), message))?,
                    },
                };
                self.push(command, settings.span())?;
            }
            if let Some(mix) = mix {
                self.push(Command::SetEffectMix { effect: id, mix }, span.clone())?;
            }
            if let Some(enabled) = enabled {
                self.push(
                    Command::SetEffectEnabled {
                        effect: id,
                        enabled,
                    },
                    span.clone(),
                )?;
            }
        }
        Ok(())
    }

    /// The notes a track's pattern plays over a section `length` long, each
    /// line repeated to fill it.
    fn pattern(
        &self,
        kind: Kind,
        pattern: &Spanned<PatternFile>,
        length: Ticks,
        bars: u32,
    ) -> Result<Vec<SectionNote>, SongError> {
        let mut lines = Vec::new();
        self.lines(kind, pattern, &mut lines)?;
        let mut notes = Vec::new();
        for PatternLine {
            line,
            step,
            length: note_length,
            span,
        } in lines
        {
            let line_length = line.steps * step;
            if line_length > length {
                return Err(self.error(
                    span,
                    format!(
                        "this line is {} steps long, longer than its section's {bars} bar{}",
                        line.steps,
                        if bars == 1 { "" } else { "s" }
                    ),
                ));
            }
            if line_length == 0 {
                return Err(self.error(span, "this line has no steps"));
            }
            for repeat in 0..length.div_ceil(line_length) {
                for hit in &line.hits {
                    let start = repeat * line_length + hit.step * step;
                    if start >= length {
                        continue;
                    }
                    notes.push(SectionNote {
                        start,
                        length: note_length.unwrap_or(hit.steps * step).min(length - start),
                        pitch: hit.pitch,
                        velocity: hit.velocity,
                    });
                }
            }
        }
        notes.sort_by_key(|note| (note.start, note.pitch));
        Ok(notes)
    }

    /// Reads a pattern's lines.
    fn lines(
        &self,
        kind: Kind,
        pattern: &Spanned<PatternFile>,
        lines: &mut Vec<PatternLine>,
    ) -> Result<(), SongError> {
        let span = pattern.span();
        match (kind, pattern.get_ref()) {
            (Kind::Synth, PatternFile::Steps(text)) => {
                let line = steps::notes(text, DEFAULT_VELOCITY)
                    .map_err(|error| self.step_error(span.clone(), text, error))?;
                lines.push(PatternLine {
                    line,
                    step: division_ticks(DEFAULT_STEP),
                    length: None,
                    span,
                });
            }
            (Kind::Drums, PatternFile::Steps(_)) => {
                return Err(self.error(
                    span,
                    "a drum pattern is a table of steps for each drum: { kick = \"x...x...\" }",
                ));
            }
            (_, PatternFile::Lines(list)) => {
                for line in list {
                    if matches!(line.get_ref(), PatternFile::Lines(_)) {
                        return Err(self.error(line.span(), "a list of lines can't hold another"));
                    }
                    self.lines(kind, line, lines)?;
                }
            }
            (Kind::Synth, PatternFile::Table(entries)) => {
                let options = self.options(entries, &["notes"], true)?;
                let notes = entries
                    .0
                    .iter()
                    .find(|(key, _)| key.get_ref() == "notes")
                    .ok_or_else(|| self.error(span.clone(), "a line's table has its `notes`"))?;
                let Value::String(text) = notes.1.get_ref() else {
                    return Err(self.error(notes.1.span(), "a line's `notes` is a string"));
                };
                let line = steps::notes(text, options.velocity)
                    .map_err(|error| self.step_error(notes.1.span(), text, error))?;
                lines.push(PatternLine {
                    line,
                    step: options.step,
                    length: options.length,
                    span: notes.1.span(),
                });
            }
            (Kind::Drums, PatternFile::Table(entries)) => {
                let rows: Vec<&str> = KIT.iter().map(|row| sound_name(row.sound)).collect();
                let options = self.options(entries, &rows, false)?;
                for (key, value) in &entries.0 {
                    let Some(row) = KIT
                        .iter()
                        .find(|row| sound_name(row.sound) == key.get_ref())
                    else {
                        continue;
                    };
                    let Value::String(text) = value.get_ref() else {
                        return Err(self.error(value.span(), "a drum's steps are a string"));
                    };
                    let line = steps::hits(text, row.pitch, options.velocity)
                        .map_err(|error| self.step_error(value.span(), text, error))?;
                    lines.push(PatternLine {
                        line,
                        step: options.step,
                        length: Some(options.step),
                        span: value.span(),
                    });
                }
            }
        }
        Ok(())
    }

    /// A line's `step`, `velocity` and, if it `has_length`, `length`, from a
    /// table whose other keys must be in `others`. A drum's hits last their
    /// step, so a drum pattern has no `length`.
    fn options(
        &self,
        entries: &Entries<Value>,
        others: &[&str],
        has_length: bool,
    ) -> Result<Options, SongError> {
        let mut options = Options {
            step: division_ticks(DEFAULT_STEP),
            length: None,
            velocity: DEFAULT_VELOCITY,
        };
        for (key, value) in &entries.0 {
            let wrong = |what: &str| {
                self.error(
                    value.span(),
                    format!("a line's `{}` is {what}", key.get_ref()),
                )
            };
            match key.get_ref().as_str() {
                "step" => {
                    options.step = value
                        .get_ref()
                        .as_str()
                        .and_then(division)
                        .map(NoteDivision::ticks)
                        .ok_or_else(|| wrong("a note length, from 1/32 to 1/1: \"1/8\", \"1/8d\" or \"1/8 dotted\", \"1/8t\""))?;
                }
                "length" if has_length => {
                    let ticks = match value.get_ref() {
                        Value::String(text) => division(text).map(NoteDivision::ticks),
                        &Value::Integer(ticks) => u64::try_from(ticks).ok().filter(|&t| t > 0),
                        _ => None,
                    };
                    options.length = Some(ticks.ok_or_else(|| {
                        wrong(
                            "a note length (\"1/8\") or a whole number of ticks (960 to a quarter)",
                        )
                    })?);
                }
                "velocity" => {
                    options.velocity = value
                        .get_ref()
                        .as_integer()
                        .and_then(|velocity| u8::try_from(velocity).ok())
                        .filter(|velocity| {
                            (Note::MIN_VELOCITY..=Note::MAX_VELOCITY).contains(velocity)
                        })
                        .ok_or_else(|| {
                            wrong(&format!(
                                "a velocity from {} to {}",
                                Note::MIN_VELOCITY,
                                Note::MAX_VELOCITY
                            ))
                        })?;
                }
                other if others.contains(&other) => {}
                other => {
                    let mut known = vec!["step"];
                    if has_length {
                        known.push("length");
                    }
                    known.push("velocity");
                    known.extend_from_slice(others);
                    return Err(self.error(
                        key.span(),
                        format!(
                            "`{other}` isn't part of a line: it has {}",
                            known.join(", ")
                        ),
                    ));
                }
            }
        }
        Ok(options)
    }

    /// A raw command, with `$track:<name>`, `$clip:<track>:<n>`,
    /// `$effect:<track>:<n>` and `$master_effect:<n>` turned into the IDs
    /// they name.
    fn raw(&mut self, raw: &Spanned<toml::Table>) -> Result<(), SongError> {
        let span = raw.span();
        let mut body = raw.get_ref().clone();
        let format = body
            .remove("format")
            .unwrap_or(Value::Integer(i64::from(COMMAND_FORMAT)));
        let mut body = Value::Table(body);
        self.substitute(&mut body)
            .map_err(|message| self.error(span.clone(), message))?;
        let mut envelope = toml::Table::new();
        envelope.insert("format".into(), format);
        envelope.insert("command".into(), body);
        let command: Command = Value::Table(envelope)
            .try_into()
            .map_err(|error| self.error(span.clone(), format!("isn't a command: {error}")))?;
        self.push(command, span)
    }

    fn substitute(&self, value: &mut Value) -> Result<(), String> {
        match value {
            Value::String(text) if text.starts_with('$') => *text = self.name(text)?,
            Value::Array(values) => {
                for value in values {
                    self.substitute(value)?;
                }
            }
            Value::Table(table) => {
                for (_, value) in table.iter_mut() {
                    self.substitute(value)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    /// The ID a `$` name names.
    fn name(&self, text: &str) -> Result<String, String> {
        let wrong = || {
            format!(
                "`{text}` isn't a name: $track:<track>, $clip:<track>:<n>, $effect:<track>:<n> or $master_effect:<n>"
            )
        };
        let (what, rest) = text[1..].split_once(':').ok_or_else(wrong)?;
        let track = |name: &str| {
            self.tracks
                .iter()
                .find(|(other, ..)| other == name)
                .ok_or_else(|| format!("`{text}`: there's no track called `{name}`"))
        };
        let nth = |rest: &str| -> Result<(String, usize), String> {
            let (name, n) = rest.rsplit_once(':').ok_or_else(wrong)?;
            let n: usize = n.parse().ok().filter(|&n| n > 0).ok_or_else(wrong)?;
            Ok((name.to_owned(), n - 1))
        };
        let missing = || format!("`{text}` names one that isn't there");
        match what {
            "track" => Ok(track(rest)?.1.to_string()),
            "clip" => {
                let (name, n) = nth(rest)?;
                let clips = &track(&name)?.2;
                clips.get(n).map(ToString::to_string).ok_or_else(missing)
            }
            "effect" => {
                let (name, n) = nth(rest)?;
                let chain = Chain::Track(track(&name)?.1);
                let effects = self.effects.get(&chain).map_or(&[][..], Vec::as_slice);
                effects.get(n).map(ToString::to_string).ok_or_else(missing)
            }
            "master_effect" => {
                let n: usize = rest.parse().ok().filter(|&n| n > 0).ok_or_else(wrong)?;
                let effects = self
                    .effects
                    .get(&Chain::Master)
                    .map_or(&[][..], Vec::as_slice);
                effects
                    .get(n - 1)
                    .map(ToString::to_string)
                    .ok_or_else(missing)
            }
            _ => Err(wrong()),
        }
    }

    /// Applies `command` to the project so far, then keeps it.
    fn push(&mut self, command: Command, span: Range<usize>) -> Result<(), SongError> {
        self.project
            .apply(&command)
            .map_err(|error| self.error(span, error.to_string()))?;
        self.commands.push(command);
        Ok(())
    }

    /// An ID worked out from the song's.
    fn derived(&self, name: &str) -> Uuid {
        Uuid::new_v5(&self.id.as_uuid(), name.as_bytes())
    }

    fn error(&self, span: Range<usize>, message: impl Into<String>) -> SongError {
        SongError {
            line: line_of(self.text, span.start),
            message: message.into(),
        }
    }

    /// A mistake in a line of steps, on the line of the file it's on.
    fn step_error(&self, span: Range<usize>, text: &str, error: steps::StepError) -> SongError {
        // The step as written is found in the file by how many times it
        // comes before, since escapes and line continuations mean the string
        // TOML read isn't the text in the file.
        let raw = &self.text[span.clone()];
        let step = text[error.offset..]
            .split_whitespace()
            .next()
            .unwrap_or_default();
        let before = text[..error.offset].matches(step).count();
        let found = if step.is_empty() {
            None
        } else {
            raw.match_indices(step).nth(before).map(|(at, _)| at)
        };
        SongError {
            line: line_of(self.text, span.start + found.unwrap_or(0)),
            message: error.message,
        }
    }
}

/// A line of a pattern, read: its hits, its step, the length every note in
/// it has if it says, and where it is in the file.
struct PatternLine {
    line: Line,
    step: Ticks,
    length: Option<Ticks>,
    span: Range<usize>,
}

/// A line's step, the length its notes have if it says, and its velocity.
struct Options {
    step: Ticks,
    length: Option<Ticks>,
    velocity: u8,
}

/// The note division `text` names: as a command has it, `"1/8d"`, or as a
/// menu does, `"1/8 dotted"`.
fn division(text: &str) -> Option<NoteDivision> {
    NoteDivision::ALL
        .into_iter()
        .find(|division| division.value() == text || division.label().eq_ignore_ascii_case(text))
}

fn division_ticks(text: &str) -> Ticks {
    division(text).expect("a division").ticks()
}

fn number(value: &Value) -> Option<f32> {
    match *value {
        Value::Float(number) => Some(number as f32),
        Value::Integer(number) => Some(number as f32),
        _ => None,
    }
}

/// A setting by name, as its command carries it: `{"name": key, "value":
/// value}`.
fn param<P: DeserializeOwned>(what: &str, key: &str, value: &Value) -> Result<P, String> {
    let mut table = toml::Table::new();
    table.insert("name".into(), Value::String(key.into()));
    table.insert("value".into(), value.clone());
    Value::Table(table)
        .try_into()
        .map_err(|error: toml::de::Error| {
            let error = error.message().trim_end().to_owned();
            if error.contains(&format!("unknown variant `{key}`")) {
                format!("`{key}` isn't a {what} setting ({error})")
            } else {
                format!("{what} setting `{key}` = {value}: {error}")
            }
        })
}

/// A kit sound's name in a song file: `kick`, `closed_hat`.
fn sound_name(sound: DrumSound) -> &'static str {
    match sound {
        DrumSound::Kick => "kick",
        DrumSound::Snare => "snare",
        DrumSound::Clap => "clap",
        DrumSound::LowTom => "low_tom",
        DrumSound::HighTom => "high_tom",
        DrumSound::ClosedHat => "closed_hat",
        DrumSound::OpenHat => "open_hat",
        DrumSound::Cymbal => "cymbal",
    }
}

fn drum_sound(name: &str) -> Result<DrumSound, String> {
    KIT.iter()
        .map(|row| row.sound)
        .find(|&sound| sound_name(sound) == name)
        .ok_or_else(|| {
            let names: Vec<_> = KIT.iter().map(|row| sound_name(row.sound)).collect();
            format!("`{name}` isn't a drum: they're {}", names.join(", "))
        })
}

#[cfg(test)]
mod tests;

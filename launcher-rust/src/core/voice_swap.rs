//! `--voice-swap`: swaps game voice lines for other recordings, in an Update folder, without
//! touching the dump (Python `tools/xvag_voice_swap.py`).
//!
//! `SpartanPort --voice-swap GAME_DIR SOURCE UPDATE_DIR [--language LANGUAGE] [--only NAMES.txt]
//! [--reference ORIGINAL] [--dry-run]`. A voice file of the game is
//! `<LANGUAGE>/SOUNDS/STREAMS_PS4/NAME.XVAG`: an XVAG header followed by PS-ADPCM frames (16 bytes
//! each, 28 samples). The new file is the original one byte for byte, with only the frames
//! replaced and the flag byte of every frame kept. A recording is refused, with the reason, when
//! that is not possible. Multichannel lines and the cinematics (MPEG) are not handled.

use std::ffi::{OsStr, OsString};
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};

use indexmap::IndexMap;

use crate::core::fs::atomic_write;
use crate::core::psarc::Psarc;

/// The fields of an XVAG header that the swap reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct XvagHeader {
    pub big_endian: bool,
    /// Where the audio starts.
    pub start: usize,
    pub channels: u32,
    pub codec: u32,
    pub samples: u32,
    pub rate: u32,
}

/// Python `read_xvag_header`.
pub fn read_xvag_header(data: &[u8]) -> Result<XvagHeader, String> {
    if data.len() < 0x44 || &data[..4] != b"XVAG" || &data[0x20..0x24] != b"fmat" {
        return Err("not an XVAG file".to_string());
    }
    let big_endian = data[8] & 1 != 0;
    let field = |at: usize| {
        let bytes = [data[at], data[at + 1], data[at + 2], data[at + 3]];
        if big_endian {
            u32::from_be_bytes(bytes)
        } else {
            u32::from_le_bytes(bytes)
        }
    };
    // Then: channels, codec, samples, samples (again), interleave factor, sample rate, data size.
    let start = field(4) as usize;
    let (channels, codec, samples, rate) = (field(0x28), field(0x2c), field(0x30), field(0x3c));
    if !(0x44..=data.len()).contains(&start) {
        return Err("XVAG audio offset is outside the payload".to_string());
    }
    if channels == 1
        && codec == 6
        && (!samples.is_multiple_of(28) || data.len() - start < samples as usize / 28 * 16)
    {
        return Err("XVAG audio is shorter than its header or has incomplete frames".to_string());
    }
    Ok(XvagHeader {
        big_endian,
        start,
        channels,
        codec,
        samples,
        rate,
    })
}

fn frames_of(bytes: &[u8]) -> Vec<[u8; 16]> {
    bytes
        .chunks_exact(16)
        .map(|chunk| {
            let mut frame = [0u8; 16];
            frame.copy_from_slice(chunk);
            frame
        })
        .collect()
}

/// Python `replacement_frames`: the PS-ADPCM frames of a mono recording (VAGp or XVAG).
pub fn replacement_frames(data: &[u8], rate: u32) -> Result<Vec<[u8; 16]>, String> {
    if data.starts_with(b"VAGp") {
        let file_rate = data
            .get(0x10..0x14)
            .map(|bytes| u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]));
        if data.len() < 0x40 || file_rate != Some(rate) {
            return Err("sample rate differs".to_string());
        }
        let body = &data[0x30..];
        if !body.len().is_multiple_of(16) {
            return Err("VAGp data is not a whole number of frames".to_string());
        }
        let frames = frames_of(body);
        if frames.len() < 3 || frames[0].iter().any(|&byte| byte != 0) {
            return Err("VAGp lacks the blank first frame".to_string());
        }
        if frames[frames.len() - 1][1] != 0x07 {
            return Err("VAGp lacks the 0x07 end frame".to_string());
        }
        return Ok(frames[1..frames.len() - 1].to_vec());
    }
    let header = read_xvag_header(data)?;
    if header.channels != 1 || header.codec != 6 {
        return Err("recording is not mono PS-ADPCM".to_string());
    }
    if header.rate != rate {
        return Err("sample rate differs".to_string());
    }
    let count = header.samples as usize / 28;
    Ok(frames_of(&data[header.start..header.start + count * 16]))
}

/// Python `swap_voice`: the original XVAG file with the frames of `recording` (same length
/// required).
pub fn swap_voice(original: &[u8], recording: &[u8]) -> Result<Vec<u8>, String> {
    let header = read_xvag_header(original)?;
    if header.channels != 1 || header.codec != 6 {
        return Err("original is not mono PS-ADPCM".to_string());
    }
    let frames = replacement_frames(recording, header.rate)?;
    let count = header.samples as usize / 28;
    if !header.samples.is_multiple_of(28) || frames.len() != count {
        return Err(format!(
            "frame count differs: recording {} vs original {count}",
            frames.len()
        ));
    }
    let mut out = original.to_vec();
    let payload = &mut out[header.start..];
    if payload.len() < count * 16 {
        return Err("original is shorter than its own header says".to_string());
    }
    for (i, frame) in frames.iter().enumerate() {
        let at = i * 16;
        // The original's flag byte (at + 1) stays.
        payload[at] = frame[0];
        payload[at + 2..at + 16].copy_from_slice(&frame[2..]);
    }
    Ok(out)
}

/// What `run` does besides its three folders.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    /// The voice folder of the game.
    pub language: String,
    /// A text file with the `NAME.XVAG` files to handle, one per line.
    pub only: Option<PathBuf>,
    /// Report only, write nothing.
    pub dry_run: bool,
    /// The unmodified archive or folder `source` was made from: recordings identical to it are
    /// left alone.
    pub reference: Option<PathBuf>,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            language: "PORTUGUESE".to_string(),
            only: None,
            dry_run: false,
            reference: None,
        }
    }
}

/// What `run` did, by file name; `refused` is keyed by the reason.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Report {
    pub written: Vec<String>,
    pub same: Vec<String>,
    pub refused: IndexMap<String, Vec<String>>,
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

/// `path` made absolute and normalised, with the deepest folder that exists in its canonical form.
fn resolve(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|dir| dir.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    };
    let mut kept = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::ParentDir => {
                kept.pop();
            }
            Component::CurDir => {}
            other => kept.push(other.as_os_str()),
        }
    }
    let mut rest = Vec::new();
    let mut base = kept.clone();
    let canonical = loop {
        if let Ok(found) = base.canonicalize() {
            break found;
        }
        match (base.file_name().map(OsStr::to_owned), base.parent()) {
            (Some(name), Some(parent)) => {
                rest.push(name);
                base = parent.to_path_buf();
            }
            _ => break kept.clone(),
        }
    };
    rest.iter()
        .rev()
        .fold(canonical, |path, name| path.join(name))
}

/// Python `is_inside`: `inner` is `outer` or below it.
fn is_inside(inner: &Path, outer: &Path) -> bool {
    resolve(inner).starts_with(resolve(outer))
}

/// The recordings of a `.psarc` or a folder, by upper-case name, read when asked for.
enum Source {
    Folder(IndexMap<String, PathBuf>),
    Archive(Psarc),
}

impl Source {
    fn open(path: &Path) -> io::Result<Source> {
        if !path.is_dir() {
            return Psarc::open(path).map(Source::Archive);
        }
        fn walk(dir: &Path, files: &mut IndexMap<String, PathBuf>) -> io::Result<()> {
            let mut paths = std::fs::read_dir(dir)?
                .map(|entry| entry.map(|entry| entry.path()))
                .collect::<io::Result<Vec<_>>>()?;
            paths.sort();
            for path in paths {
                if path.is_dir() {
                    walk(&path, files)?;
                } else if let Some(name) = path.file_name() {
                    files.insert(name.to_string_lossy().to_uppercase(), path);
                }
            }
            Ok(())
        }
        let mut files = IndexMap::new();
        walk(path, &mut files)?;
        Ok(Source::Folder(files))
    }

    fn has(&self, name: &str) -> bool {
        match self {
            Source::Folder(files) => files.contains_key(name),
            Source::Archive(archive) => archive.names().contains_key(name),
        }
    }

    /// The recording called `name` (upper case), if there is one.
    fn read(&mut self, name: &str) -> io::Result<Option<Vec<u8>>> {
        match self {
            Source::Folder(files) => files.get(name).map(std::fs::read).transpose(),
            Source::Archive(archive) => match archive.names().get(name).copied() {
                Some(index) => archive.read(index).map(Some),
                None => Ok(None),
            },
        }
    }
}

/// Python `find_file`: `relative` (with `/` separators) inside `folder`, matching names without
/// regard to case.
fn find_file(folder: &Path, relative: &str) -> Option<PathBuf> {
    let mut path = folder.to_path_buf();
    for part in relative.split('/') {
        if !path.is_dir() {
            return None;
        }
        let wanted = part.to_lowercase();
        let mut names = std::fs::read_dir(&path)
            .ok()?
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .collect::<Vec<_>>();
        names.sort();
        path = names.into_iter().find(|candidate| {
            candidate
                .file_name()
                .is_some_and(|name| name.to_string_lossy().to_lowercase() == wanted)
        })?;
    }
    path.is_file().then_some(path)
}

/// Python `run`: writes the swapped files to `update`. Errors the Python raised as `ValueError`
/// are `InvalidInput` here, with the same text.
pub fn run(game: &Path, source: &Path, update: &Path, options: &Options) -> io::Result<Report> {
    if !game.is_dir() {
        return Err(invalid(format!("{}: not a folder", game.display())));
    }
    if is_inside(update, game) || is_inside(game, update) {
        return Err(invalid(
            "UPDATE_DIR and GAME_DIR must be two separate folders",
        ));
    }
    let language = options.language.to_uppercase();
    let mut language_dir = None;
    for entry in std::fs::read_dir(game)? {
        let path = entry?.path();
        if path.is_dir()
            && path
                .file_name()
                .is_some_and(|name| name.to_string_lossy().to_uppercase() == language)
        {
            language_dir = Some(path);
            break;
        }
    }
    let voices = language_dir
        .map(|dir| dir.join("SOUNDS").join("STREAMS_PS4"))
        .filter(|dir| dir.is_dir())
        .ok_or_else(|| {
            invalid(format!(
                "{}: no {}/SOUNDS/STREAMS_PS4 folder",
                game.display(),
                options.language
            ))
        })?;
    let wanted: Option<Vec<String>> = match &options.only {
        Some(only) => Some(
            std::fs::read_to_string(only)?
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_uppercase)
                .collect(),
        ),
        None => None,
    };
    let mut recordings = Source::open(source)?;
    let mut untouched = options.reference.as_deref().map(Source::open).transpose()?;
    let mut report = Report::default();
    let target = update.join(voices.strip_prefix(game).unwrap_or(&voices));
    let mut originals = std::fs::read_dir(&voices)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<io::Result<Vec<_>>>()?;
    originals.retain(|path| path.is_file());
    originals.sort();
    for original in originals {
        let Some(file_name) = original.file_name() else {
            continue;
        };
        let file_name = file_name.to_string_lossy().into_owned();
        let name = file_name.to_uppercase();
        if !recordings.has(&name)
            || wanted
                .as_ref()
                .is_some_and(|wanted| !wanted.contains(&name))
        {
            continue;
        }
        let old = std::fs::read(&original)?;
        let Some(new_audio) = recordings.read(&name)? else {
            continue;
        };
        let unchanged = match untouched.as_mut() {
            Some(reference) if reference.has(&name) => {
                reference.read(&name)?.as_deref() == Some(new_audio.as_slice())
            }
            _ => false,
        };
        if new_audio == old || unchanged {
            report.same.push(file_name);
            continue;
        }
        let new = match swap_voice(&old, &new_audio) {
            Ok(new) => new,
            Err(error) => {
                let reason = error.split(':').next().unwrap_or_default().to_string();
                report.refused.entry(reason).or_default().push(file_name);
                continue;
            }
        };
        if new == old {
            report.same.push(file_name);
            continue;
        }
        if !options.dry_run {
            std::fs::create_dir_all(&target)?;
            atomic_write(&target.join(&file_name), &new)?;
        }
        report.written.push(file_name);
    }
    if !report.written.is_empty() && !options.dry_run {
        // The launcher accepts an Update folder only with the game's eboot.bin and
        // sce_sys/param.sfo (identical copies).
        for relative in ["eboot.bin", "sce_sys/param.sfo"] {
            if let Some(found) = find_file(game, relative) {
                let copy = update.join(relative);
                if !copy.exists() {
                    if let Some(parent) = copy.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::copy(&found, &copy)?;
                }
            }
        }
    }
    Ok(report)
}

const USAGE: &str = "usage: SpartanPort --voice-swap GAME_DIR SOURCE UPDATE_DIR \
[--language LANGUAGE] [--only NAMES.txt] [--reference ORIGINAL] [--dry-run]\n";

const HELP: &str = "
Swaps game voice lines for other recordings, in an Update folder, without touching the dump.

positional arguments:
  GAME_DIR              the dumped game (the folder with eboot.bin), read only
  SOURCE                a PS3 language archive (.psarc) or a folder of recordings named like the game's voice files
  UPDATE_DIR            an empty or new folder to set as the Update folder; never inside GAME_DIR

options:
  -h, --help            show this help message and exit
  --language LANGUAGE   voice folder of the game (default PORTUGUESE)
  --only NAMES.txt      text file with the NAME.XVAG files to handle, one per line
  --reference ORIGINAL  the unmodified archive or folder SOURCE was made from: recordings identical to it are left alone
  --dry-run             report only, write nothing
";

/// The command line, as the Python `argparse` read it.
struct Arguments {
    game: OsString,
    source: OsString,
    update: OsString,
    options: Options,
    typed_only: Option<OsString>,
    typed_reference: Option<OsString>,
}

fn parse(args: &[OsString]) -> Result<Option<Arguments>, String> {
    let mut positional: Vec<OsString> = Vec::new();
    let mut extra: Vec<String> = Vec::new();
    let mut options = Options::default();
    let (mut typed_only, mut typed_reference) = (None, None);
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        let text = arg.to_string_lossy().into_owned();
        let (flag, inline) = match text.split_once('=') {
            Some((flag, value)) if flag.starts_with("--") => (flag.to_string(), Some(value)),
            _ => (text.clone(), None),
        };
        match flag.as_str() {
            "-h" | "--help" => return Ok(None),
            "--dry-run" => options.dry_run = true,
            "--language" | "--only" | "--reference" => {
                let value: OsString = match inline {
                    Some(value) => OsString::from(value),
                    None => match iter.next() {
                        Some(value) if !value.to_string_lossy().starts_with('-') => value.clone(),
                        _ => return Err(format!("argument {flag}: expected one argument")),
                    },
                };
                match flag.as_str() {
                    "--language" => options.language = value.to_string_lossy().into_owned(),
                    "--only" => typed_only = Some(value),
                    _ => typed_reference = Some(value),
                }
            }
            _ if text.starts_with('-') && text.len() > 1 => extra.push(text),
            _ => positional.push(arg.clone()),
        }
    }
    if positional.len() < 3 {
        let missing = ["game", "source", "update"][positional.len()..].join(", ");
        return Err(format!("the following arguments are required: {missing}"));
    }
    extra.extend(
        positional
            .split_off(3)
            .iter()
            .map(|arg| arg.to_string_lossy().into_owned()),
    );
    if !extra.is_empty() {
        return Err(format!("unrecognized arguments: {}", extra.join(" ")));
    }
    let mut positional = positional.into_iter();
    let mut next = || positional.next().unwrap_or_default();
    Ok(Some(Arguments {
        game: next(),
        source: next(),
        update: next(),
        options,
        typed_only,
        typed_reference,
    }))
}

/// `--voice-swap`: reads `args` (what follows the flag), prints the report to `out` and the
/// problems to `err`; returns the exit code. A relative path is joined to `base` (the folder the
/// player typed it in, when the AppImage runs from its mount).
pub fn main(
    args: &[OsString],
    base: Option<&Path>,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> i32 {
    let arguments = match parse(args) {
        Ok(None) => {
            let _ = write!(out, "{USAGE}{HELP}");
            return 0;
        }
        Ok(Some(arguments)) => arguments,
        Err(message) => {
            let _ = writeln!(err, "{USAGE}voice-swap: error: {message}");
            return 2;
        }
    };
    let at = |typed: &OsStr| match base {
        Some(base) => base.join(typed),
        None => PathBuf::from(typed),
    };
    let mut options = arguments.options;
    options.only = arguments.typed_only.as_deref().map(at);
    options.reference = arguments.typed_reference.as_deref().map(at);
    let report = match run(
        &at(&arguments.game),
        &at(&arguments.source),
        &at(&arguments.update),
        &options,
    ) {
        Ok(report) => report,
        Err(error) => {
            let _ = writeln!(err, "voice-swap: {error}");
            return 1;
        }
    };
    let verb = if options.dry_run {
        "would be written"
    } else {
        "written"
    };
    let _ = writeln!(
        out,
        "{} files {verb} to {}",
        report.written.len(),
        Path::new(&arguments.update).display()
    );
    let _ = writeln!(
        out,
        "{} lines skipped: the recording is the original audio",
        report.same.len()
    );
    let mut refused: Vec<_> = report.refused.iter().collect();
    refused.sort_by_key(|(_, names)| std::cmp::Reverse(names.len()));
    for (reason, names) in refused {
        let examples: Vec<&str> = names.iter().take(3).map(String::as_str).collect();
        let _ = writeln!(
            out,
            "{} refused ({reason}), e.g. {}",
            names.len(),
            examples.join(", ")
        );
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::psarc;

    /// A fake PS-ADPCM frame: shift/predictor byte, flag byte, 14 data bytes.
    fn frame(seed: u8, flag: u8) -> [u8; 16] {
        let mut out = [0u8; 16];
        out[0] = 0x0c;
        out[1] = flag;
        for i in 0..14 {
            out[2 + i] = seed.wrapping_add(i as u8);
        }
        out
    }

    fn original() -> Vec<[u8; 16]> {
        (0..4)
            .map(|i| frame(10 * i, if i == 3 { 1 } else { 0 }))
            .collect()
    }

    fn dubbed() -> Vec<[u8; 16]> {
        (0..4).map(|i| frame(100 + 10 * i, 0)).collect()
    }

    fn word(big: bool, value: u32) -> [u8; 4] {
        if big {
            value.to_be_bytes()
        } else {
            value.to_le_bytes()
        }
    }

    /// A mono XVAG file made of `frames`, laid out like the game's own (0x20 preamble, fmat, end
    /// chunk).
    fn xvag_with(
        frames: &[[u8; 16]],
        big: bool,
        padding: usize,
        channels: u32,
        codec: u32,
        rate: u32,
    ) -> Vec<u8> {
        let samples = frames.len() as u32 * 28;
        let mut data = frames.concat();
        data.extend(vec![0u8; padding]);
        let mut header = b"XVAG".to_vec();
        header.extend(word(big, 0x4c));
        header.extend([if big { 1 } else { 2 }, 0, 0, 0x60]);
        header.extend([0u8; 20]);
        header.extend(b"fmat");
        header.extend(word(big, 0x1c));
        for value in [
            channels,
            codec,
            samples,
            samples,
            1,
            rate,
            data.len() as u32,
        ] {
            header.extend(word(big, value));
        }
        header.extend(b"0000");
        header.extend([0u8; 4]);
        assert_eq!(header.len(), 0x4c);
        header.extend(data);
        header
    }

    fn xvag(frames: &[[u8; 16]], big: bool) -> Vec<u8> {
        xvag_with(frames, big, 0, 1, 6, 48000)
    }

    /// A Sony VAG file: 0x30 byte header, blank first frame, the frames, 0x07 end frame.
    fn vagp_with(frames: &[[u8; 16]], rate: u32, blank: bool, end: bool) -> Vec<u8> {
        let mut body = Vec::new();
        if blank {
            body.extend([0u8; 16]);
        }
        body.extend(frames.concat());
        if end {
            body.extend([0, 7]);
            body.extend([0u8; 14]);
        }
        let mut out = b"VAGp".to_vec();
        for value in [0x20u32, 0, body.len() as u32, rate] {
            out.extend(value.to_be_bytes());
        }
        out.extend([0u8; 12]);
        out.extend([0u8; 16]);
        out.extend(body);
        out
    }

    fn vagp(frames: &[[u8; 16]]) -> Vec<u8> {
        vagp_with(frames, 48000, true, true)
    }

    #[test]
    fn only_the_frames_change() {
        for big in [true, false] {
            let (old, recording) = (xvag(&original(), big), vagp(&dubbed()));
            let new = swap_voice(&old, &recording).unwrap();
            assert_eq!(new.len(), old.len());
            assert_eq!(new[..0x4c], old[..0x4c]); // header untouched
            for (i, (old_frame, dub)) in original().iter().zip(dubbed()).enumerate() {
                let got = &new[0x4c + 16 * i..0x4c + 16 * i + 16];
                assert_eq!(got[0], dub[0]);
                assert_eq!(got[1], old_frame[1]); // the original's flag byte stays
                assert_eq!(got[2..], dub[2..]);
            }
        }
    }

    #[test]
    fn padding_after_the_frames_is_kept() {
        let old = xvag_with(&original(), false, 40, 1, 6, 48000);
        let new = swap_voice(&old, &vagp(&dubbed())).unwrap();
        assert_eq!(new.len(), old.len());
        assert_eq!(new[new.len() - 40..], [0u8; 40]);
    }

    #[test]
    fn an_xvag_recording_works_like_a_vagp_one() {
        let recording = xvag_with(&dubbed(), true, 7, 1, 6, 48000);
        let from_xvag = swap_voice(&xvag(&original(), false), &recording).unwrap();
        let from_vagp = swap_voice(&xvag(&original(), false), &vagp(&dubbed())).unwrap();
        assert_eq!(from_xvag, from_vagp);
    }

    #[test]
    fn truncated_xvag_recordings_are_refused() {
        for missing in [1, 16, 64] {
            let recording = xvag(&dubbed(), true);
            let cut = &recording[..recording.len() - missing];
            assert!(
                swap_voice(&xvag(&original(), true), cut).is_err(),
                "{missing}"
            );
        }
    }

    #[test]
    fn xvag_payload_cannot_overlap_the_header() {
        let mut malformed = xvag(&original(), true);
        malformed[4..8].copy_from_slice(&0u32.to_be_bytes());
        assert!(swap_voice(&malformed, &vagp(&dubbed())).is_err());
        let mut recording = xvag(&dubbed(), true);
        recording[4..8].copy_from_slice(&0u32.to_be_bytes());
        assert!(swap_voice(&xvag(&original(), true), &recording).is_err());
    }

    #[test]
    fn what_does_not_fit_is_refused_with_a_reason() {
        let old = xvag(&original(), true);
        let mut longer = dubbed();
        longer.push(frame(1, 0));
        let cases: Vec<(&str, Vec<u8>)> = vec![
            ("frame count differs", vagp(&longer)),
            (
                "sample rate differs",
                vagp_with(&dubbed(), 44100, true, true),
            ),
            (
                "VAGp lacks the blank first frame",
                vagp_with(&dubbed(), 48000, false, true),
            ),
            (
                "VAGp lacks the 0x07 end frame",
                vagp_with(&dubbed(), 48000, true, false),
            ),
            (
                "recording is not mono PS-ADPCM",
                xvag_with(&dubbed(), true, 0, 2, 6, 48000),
            ),
        ];
        for (reason, recording) in cases {
            let error = swap_voice(&old, &recording).unwrap_err();
            assert!(error.starts_with(reason), "{reason}: {error}");
        }
        let bad = [
            xvag_with(&original(), true, 0, 2, 6, 48000),
            xvag_with(&original(), true, 0, 1, 8, 48000),
            b"not an xvag file".repeat(8),
        ];
        for bad in bad {
            assert!(swap_voice(&bad, &vagp(&dubbed())).is_err());
        }
    }

    /// The Python `RunTests.setUp` tree.
    struct Tree {
        root: tempfile::TempDir,
        game: PathBuf,
        update: PathBuf,
        source: PathBuf,
    }

    impl Tree {
        fn new() -> Tree {
            let root = tempfile::tempdir().unwrap();
            let (game, update, source) = (
                root.path().join("game"),
                root.path().join("update"),
                root.path().join("source"),
            );
            let voices = game.join("PORTUGUESE/SOUNDS/STREAMS_PS4");
            std::fs::create_dir_all(&voices).unwrap();
            std::fs::write(game.join("eboot.bin"), b"eboot").unwrap();
            std::fs::create_dir(game.join("sce_sys")).unwrap();
            std::fs::write(game.join("sce_sys/param.sfo"), b"sfo").unwrap();
            for name in ["A.XVAG", "B.XVAG", "C.XVAG"] {
                std::fs::write(voices.join(name), xvag(&original(), true)).unwrap();
            }
            std::fs::create_dir(voices.join("CINEMATICS")).unwrap();
            std::fs::write(voices.join("CINEMATICS/A.XVAG"), b"cinematic").unwrap();
            std::fs::create_dir(&source).unwrap();
            let mut wrong = dubbed();
            wrong.push(frame(9, 0));
            std::fs::write(source.join("a.xvag"), vagp(&dubbed())).unwrap(); // fits
            std::fs::write(source.join("b.xvag"), xvag(&original(), true)).unwrap(); // the original
            std::fs::write(source.join("c.xvag"), vagp(&wrong)).unwrap(); // wrong length
            std::fs::write(source.join("other.xvag"), vagp(&dubbed())).unwrap(); // not a line
            Tree {
                root,
                game,
                update,
                source,
            }
        }

        fn snapshot(&self) -> Vec<(PathBuf, Vec<u8>)> {
            fn walk(dir: &Path, out: &mut Vec<(PathBuf, Vec<u8>)>) {
                let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
                    .unwrap()
                    .map(|e| e.unwrap().path())
                    .collect();
                paths.sort();
                for path in paths {
                    if path.is_dir() {
                        walk(&path, out);
                    } else {
                        out.push((path.clone(), std::fs::read(&path).unwrap()));
                    }
                }
            }
            let mut out = Vec::new();
            walk(&self.game, &mut out);
            out
        }
    }

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn writes_the_fitting_lines_to_the_update_folder_and_leaves_the_game_alone() {
        let tree = Tree::new();
        let before = tree.snapshot();
        let report = run(&tree.game, &tree.source, &tree.update, &Options::default()).unwrap();
        assert_eq!(report.written, names(&["A.XVAG"]));
        assert_eq!(report.same, names(&["B.XVAG"]));
        assert_eq!(report.refused.len(), 1);
        assert_eq!(report.refused["frame count differs"], names(&["C.XVAG"]));
        assert_eq!(tree.snapshot(), before);
        let voices = tree.update.join("PORTUGUESE/SOUNDS/STREAMS_PS4");
        let listed: Vec<_> = std::fs::read_dir(&voices)
            .unwrap()
            .map(|e| e.unwrap().file_name())
            .collect();
        assert_eq!(listed, vec![OsString::from("A.XVAG")]);
        assert_eq!(
            std::fs::read(voices.join("A.XVAG")).unwrap(),
            swap_voice(&xvag(&original(), true), &vagp(&dubbed())).unwrap()
        );
        // The launcher needs both.
        assert_eq!(
            std::fs::read(tree.update.join("eboot.bin")).unwrap(),
            b"eboot"
        );
        assert_eq!(
            std::fs::read(tree.update.join("sce_sys/param.sfo")).unwrap(),
            b"sfo"
        );
    }

    #[test]
    fn dry_run_writes_nothing() {
        let tree = Tree::new();
        let options = Options {
            dry_run: true,
            ..Options::default()
        };
        let report = run(&tree.game, &tree.source, &tree.update, &options).unwrap();
        assert_eq!(report.written, names(&["A.XVAG"]));
        assert!(!tree.update.exists());
    }

    #[test]
    fn reference_leaves_unchanged_recordings_alone() {
        let tree = Tree::new();
        let reference = tree.root.path().join("reference");
        std::fs::create_dir(&reference).unwrap();
        // The same as the source: this line was not redubbed.
        std::fs::write(reference.join("a.xvag"), vagp(&dubbed())).unwrap();
        let options = Options {
            reference: Some(reference),
            ..Options::default()
        };
        let report = run(&tree.game, &tree.source, &tree.update, &options).unwrap();
        assert!(report.written.is_empty());
        assert!(report.same.contains(&"A.XVAG".to_string()));
    }

    #[test]
    fn only_limits_the_lines() {
        let tree = Tree::new();
        let only = tree.root.path().join("names.txt");
        std::fs::write(&only, "c.xvag\n").unwrap();
        let options = Options {
            only: Some(only),
            ..Options::default()
        };
        let report = run(&tree.game, &tree.source, &tree.update, &options).unwrap();
        assert!(report.written.is_empty());
        assert_eq!(
            report.refused.keys().collect::<Vec<_>>(),
            ["frame count differs"]
        );
    }

    #[test]
    fn the_update_folder_cannot_be_the_game_or_inside_it() {
        let tree = Tree::new();
        for folder in [tree.game.clone(), tree.game.join("new")] {
            let error = run(&tree.game, &tree.source, &folder, &Options::default()).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
            assert_eq!(
                error.to_string(),
                "UPDATE_DIR and GAME_DIR must be two separate folders"
            );
        }
        assert!(!tree.game.join("new").exists());
        // And the other way round: the game inside the update folder.
        let error = run(
            &tree.game,
            &tree.source,
            tree.root.path(),
            &Options::default(),
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn a_psarc_source_is_read_by_name_whatever_the_entry_order() {
        let tree = Tree::new();
        let dub = vagp(&dubbed());
        let archive = tree.root.path().join("dub.psarc");
        std::fs::write(
            &archive,
            psarc::build(
                &[
                    ("/SOUNDS/STREAMS/A.XVAG", &dub),
                    ("/SOUNDS/STREAMS/B.XVAG", &dub),
                    ("/SOUNDS/STREAMS/CINEMATICS/Z.XVAG", b"zz"),
                ],
                true,
                65536,
            ),
        )
        .unwrap();
        let report = run(&tree.game, &archive, &tree.update, &Options::default()).unwrap();
        let mut written = report.written.clone();
        written.sort();
        assert_eq!(written, names(&["A.XVAG", "B.XVAG"]));
        assert_eq!(
            std::fs::read(tree.update.join("PORTUGUESE/SOUNDS/STREAMS_PS4/B.XVAG")).unwrap(),
            swap_voice(&xvag(&original(), true), &dub).unwrap()
        );
    }

    fn args(list: &[&Path]) -> Vec<OsString> {
        list.iter().map(|p| p.as_os_str().to_owned()).collect()
    }

    #[test]
    fn cli_reports_and_returns_zero() {
        let tree = Tree::new();
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let mut given = args(&[&tree.game, &tree.source, &tree.update]);
        given.push("--dry-run".into());
        assert_eq!(main(&given, None, &mut out, &mut err), 0);
        let out = String::from_utf8(out).unwrap();
        assert!(
            out.contains(&format!(
                "1 files would be written to {}",
                tree.update.display()
            )),
            "{out}"
        );
        assert!(
            out.contains("1 lines skipped: the recording is the original audio"),
            "{out}"
        );
        assert!(
            out.contains("1 refused (frame count differs), e.g. C.XVAG"),
            "{out}"
        );
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let inside = tree.game.join("x");
        let given = args(&[&tree.game, &tree.game, &inside]);
        assert_eq!(main(&given, None, &mut out, &mut err), 1);
        assert!(String::from_utf8(err).unwrap().starts_with("voice-swap: "));
    }

    #[test]
    fn relative_paths_follow_the_appimages_folder() {
        let tree = Tree::new();
        let (mut out, mut err) = (Vec::new(), Vec::new());
        let given: Vec<OsString> = ["game", "source", "update"].map(OsString::from).to_vec();
        assert_eq!(
            main(&given, Some(tree.root.path()), &mut out, &mut err),
            0,
            "{}",
            String::from_utf8_lossy(&err)
        );
        assert!(tree
            .root
            .path()
            .join("update/PORTUGUESE/SOUNDS/STREAMS_PS4/A.XVAG")
            .exists());
        assert!(String::from_utf8(out).unwrap().contains("to update"));
    }

    #[test]
    fn bad_arguments_print_the_usage() {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        assert_eq!(main(&["game".into()], None, &mut out, &mut err), 2);
        let err = String::from_utf8(err).unwrap();
        assert!(err.contains("usage: SpartanPort --voice-swap"), "{err}");
        assert!(err.contains("voice-swap: error: "), "{err}");
        assert!(out.is_empty());
    }

    #[test]
    fn help_prints_the_usage_and_returns_zero() {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        assert_eq!(main(&["--help".into()], None, &mut out, &mut err), 0);
        let out = String::from_utf8(out).unwrap();
        assert!(out.starts_with(
            "usage: SpartanPort --voice-swap GAME_DIR SOURCE UPDATE_DIR [--language LANGUAGE] \
             [--only NAMES.txt] [--reference ORIGINAL] [--dry-run]"
        ));
        assert!(out.contains("report only, write nothing"));
        assert!(err.is_empty());
    }
}

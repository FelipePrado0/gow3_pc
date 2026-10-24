//! The AppContent profile (Python `scripts/content_profile.py`): the SKU and the four user-defined
//! parameters of `param.sfo`, as `content.bin` (`G3CONT01`, read by `probe.c`) and
//! `content-profile.json`. The SKU is a probe setting, not a license check.

use std::io;
use std::path::{Path, PathBuf};
use std::str::FromStr;

use serde::ser::Serializer;
use serde::Serialize;

use crate::core::fs::atomic_write;
use crate::core::patches::ensure_ascii;
use crate::core::sfo::{load_sfo, SfoError, SfoReadError, SfoValue};

const MAGIC: &[u8; 8] = b"G3CONT01";
const SKU_SOURCE: &str = "explicit probe setting";

/// Which edition the game reports to `sceAppContent`: `full` is stored as 3, `trial` as 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Sku {
    Full,
    Trial,
}

impl Sku {
    fn code(self) -> u32 {
        match self {
            Sku::Full => 3,
            Sku::Trial => 1,
        }
    }
}

impl FromStr for Sku {
    type Err = ContentError;

    /// The `--sku` choices of the Python: `full` or `trial`.
    fn from_str(text: &str) -> Result<Self, ContentError> {
        match text {
            "full" => Ok(Sku::Full),
            "trial" => Ok(Sku::Trial),
            other => Err(ContentError::BadSku(other.to_string())),
        }
    }
}

/// Why the profile could not be written. The texts are the Python's where it had them.
#[derive(Debug, thiserror::Error)]
pub enum ContentError {
    #[error("invalid user-defined parameter")]
    InvalidParameter,
    #[error("invalid choice: '{0}' (choose from 'full', 'trial')")]
    BadSku(String),
    #[error(transparent)]
    Sfo(#[from] SfoError),
    #[error("{}: {source}", path.display())]
    Io { path: PathBuf, source: io::Error },
}

/// What `content-profile.json` holds, in the Python's key order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Profile {
    /// `TITLE_ID` of the `param.sfo`; absent is `null`.
    #[serde(serialize_with = "sfo_value")]
    pub title_id: Option<SfoValue>,
    pub sku: Sku,
    pub sku_source: &'static str,
    /// `USER_DEFINED_PARAM_1` to `_4`; 0 when the `param.sfo` lacks one.
    pub user_params: [u32; 4],
    pub mounted_addons: Vec<String>,
    pub boot_attr: u32,
}

fn sfo_value<S: Serializer>(value: &Option<SfoValue>, serializer: S) -> Result<S::Ok, S::Error> {
    match value {
        None => serializer.serialize_none(),
        Some(SfoValue::Int(number)) => serializer.serialize_u32(*number),
        Some(SfoValue::Text(text)) => serializer.serialize_str(text),
    }
}

fn io_error(path: &Path) -> impl FnOnce(io::Error) -> ContentError + '_ {
    move |source| ContentError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// Writes `content.bin` and `content-profile.json` in `out` from `<game>/sce_sys/param.sfo`.
pub fn prepare(game: &Path, out: &Path, sku: Sku) -> Result<Profile, ContentError> {
    let path = game.join("sce_sys").join("param.sfo");
    let values = load_sfo(&path).map_err(|error| match error {
        SfoReadError::Io(source) => io_error(&path)(source),
        SfoReadError::Invalid(error) => ContentError::Sfo(error),
    })?;
    let mut user_params = [0u32; 4];
    for (i, slot) in user_params.iter_mut().enumerate() {
        match values.get(&format!("USER_DEFINED_PARAM_{}", i + 1)) {
            None => {}
            Some(SfoValue::Int(number)) => *slot = *number,
            Some(SfoValue::Text(_)) => return Err(ContentError::InvalidParameter),
        }
    }
    let profile = Profile {
        title_id: values.get("TITLE_ID").cloned(),
        sku,
        sku_source: SKU_SOURCE,
        user_params,
        mounted_addons: Vec::new(),
        boot_attr: 0,
    };
    let mut bin = MAGIC.to_vec();
    for value in std::iter::once(sku.code()).chain(user_params) {
        bin.extend_from_slice(&value.to_le_bytes());
    }
    let bin_path = out.join("content.bin");
    atomic_write(&bin_path, &bin).map_err(io_error(&bin_path))?;
    let json_path = out.join("content-profile.json");
    let mut text = serde_json::to_string_pretty(&profile)
        .map_err(|error| io_error(&json_path)(io::Error::other(error)))?;
    text = ensure_ascii(&text);
    text.push('\n');
    atomic_write(&json_path, text.as_bytes()).map_err(io_error(&json_path))?;
    Ok(profile)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// A `param.sfo` holding `entries`: `(key, fmt, value bytes)` each.
    fn sfo(entries: &[(&str, u16, Vec<u8>)]) -> Vec<u8> {
        let mut keys = Vec::new();
        let mut values = Vec::<u8>::new();
        let mut table = Vec::new();
        for (key, fmt, value) in entries {
            table.extend((keys.len() as u16).to_le_bytes());
            table.extend(fmt.to_le_bytes());
            table.extend((value.len() as u32).to_le_bytes());
            table.extend((value.len() as u32).to_le_bytes());
            table.extend((values.len() as u32).to_le_bytes());
            keys.extend(key.bytes());
            keys.push(0);
            values.extend(value.iter());
        }
        let key_table = 20 + table.len();
        let mut data = b"\0PSF\x01\x01\0\0".to_vec();
        data.extend((key_table as u32).to_le_bytes());
        data.extend(((key_table + keys.len()) as u32).to_le_bytes());
        data.extend((entries.len() as u32).to_le_bytes());
        data.extend(table);
        data.extend(keys);
        data.extend(values);
        data
    }

    fn int(key: &'static str, value: u32) -> (&'static str, u16, Vec<u8>) {
        (key, 0x404, value.to_le_bytes().to_vec())
    }

    fn game(entries: &[(&str, u16, Vec<u8>)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("sce_sys")).unwrap();
        fs::write(dir.path().join("sce_sys/param.sfo"), sfo(entries)).unwrap();
        dir
    }

    #[test]
    fn sfo_parameters_and_explicit_trial_profile() {
        let dir = game(&[
            int("USER_DEFINED_PARAM_1", 13),
            int("USER_DEFINED_PARAM_4", 0xffff_ffff),
        ]);
        let profile = prepare(dir.path(), dir.path(), Sku::Trial).unwrap();
        assert_eq!(profile.user_params, [13, 0, 0, 0xffff_ffff]);
        let mut expected = b"G3CONT01".to_vec();
        for value in [1u32, 13, 0, 0, 0xffff_ffff] {
            expected.extend(value.to_le_bytes());
        }
        assert_eq!(fs::read(dir.path().join("content.bin")).unwrap(), expected);
    }

    #[test]
    fn invalid_sfo_parameter_rejected() {
        let dir = game(&[("USER_DEFINED_PARAM_1", 0x204, b"13".to_vec())]);
        let error = prepare(dir.path(), dir.path(), Sku::Full).unwrap_err();
        assert!(
            error.to_string().contains("invalid user-defined"),
            "{error}"
        );
        assert!(!dir.path().join("content.bin").exists());
    }

    #[test]
    fn a_hostile_param_sfo_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("sce_sys")).unwrap();
        let mut hostile = sfo(&[int("USER_DEFINED_PARAM_1", 1)]);
        hostile.truncate(30);
        for bytes in [Vec::new(), b"junk".to_vec(), hostile] {
            fs::write(dir.path().join("sce_sys/param.sfo"), bytes).unwrap();
            assert!(prepare(dir.path(), dir.path(), Sku::Full).is_err());
        }
        fs::remove_file(dir.path().join("sce_sys/param.sfo")).unwrap();
        let error = prepare(dir.path(), dir.path(), Sku::Full).unwrap_err();
        assert!(matches!(error, ContentError::Io { .. }), "{error}");
    }

    #[test]
    fn a_missing_title_id_is_null_and_a_non_ascii_one_is_escaped() {
        let dir = game(&[]);
        prepare(dir.path(), dir.path(), Sku::Full).unwrap();
        let text = fs::read_to_string(dir.path().join("content-profile.json")).unwrap();
        assert!(text.contains("\"title_id\": null"), "{text}");
        let dir = game(&[("TITLE_ID", 0x204, "Ä😀".as_bytes().to_vec())]);
        prepare(dir.path(), dir.path(), Sku::Full).unwrap();
        let text = fs::read_to_string(dir.path().join("content-profile.json")).unwrap();
        assert!(text.contains("\"\\u00c4\\ud83d\\ude00\""), "{text}");
    }

    #[test]
    fn a_del_in_the_title_id_is_escaped_as_python_does() {
        // json.dumps escapes every character outside ' '..'~', DEL (0x7f) too.
        let dir = game(&[("TITLE_ID", 0x204, b"A\x7fB".to_vec())]);
        prepare(dir.path(), dir.path(), Sku::Full).unwrap();
        let text = fs::read_to_string(dir.path().join("content-profile.json")).unwrap();
        assert!(text.contains("\"title_id\": \"A\\u007fB\""), "{text:?}");
    }

    #[test]
    fn the_sku_text_is_full_or_trial() {
        assert_eq!("full".parse::<Sku>().unwrap(), Sku::Full);
        assert_eq!("trial".parse::<Sku>().unwrap(), Sku::Trial);
        let error = "demo".parse::<Sku>().unwrap_err();
        assert!(error.to_string().contains("'demo'"), "{error}");
    }
}

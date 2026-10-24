//! The update view (Python `scripts/game_update.py`): a read-only combined view of a dumped base
//! game and its separate update folder.
//!
//! The update (eboot.bin, sce_sys, ...) is laid over the base game in a temporary folder, so
//! neither dump is modified. Whatever the update does not touch is linked as a whole (a symlink, or
//! a junction / hard link on Windows without the symlink privilege, see `mods::make_link`), so a
//! 47 GB game costs a handful of links. Names are matched without regard to case.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::core::mods::{every_way, remove_tree, resolve, LinkKind, Linker};
use crate::core::patches::game_title_id;

/// The views `create_view` makes in `out`: `game-update-XXXXXX`.
const VIEW_PREFIX: &str = "game-update-";

/// Why the view could not be made. The texts are the Python's.
#[derive(Debug, thiserror::Error)]
pub enum UpdateViewError {
    /// The two `TITLE_ID`s differ, or the base has none (`None` when unreadable, as the Python
    /// prints it).
    #[error("Game and update are not the same title: {base} / {update}")]
    DifferentTitles { base: String, update: String },
    #[error("Choose two separate folders for the game and its update")]
    NestedFolders,
    #[error("The update folder needs eboot.bin and sce_sys/param.sfo")]
    NoEboot,
    #[error(transparent)]
    Io(#[from] io::Error),
}

/// Lays `source` over `dest`: its files replace dest's, folders are merged, names are
/// case-insensitive. `dest` holds links to the base; a folder both have is made a real one.
pub fn overlay(source: &Path, dest: &Path) -> io::Result<()> {
    overlay_into(&mut Linker::new(&every_way), source, dest, &mut Vec::new())
}

/// `overlay`, refusing a source folder that leads back into one being merged (it would be merged
/// forever); `ancestors` are the source folders on the way, resolved.
fn overlay_into(
    linker: &mut Linker<'_>,
    source: &Path,
    dest: &Path,
    ancestors: &mut Vec<PathBuf>,
) -> io::Result<()> {
    let real = fs::canonicalize(source)?;
    if ancestors.contains(&real) {
        return Err(io::Error::other(format!(
            "{}: the folder leads back into itself",
            source.display()
        )));
    }
    ancestors.push(real);
    let result = merge(linker, source, dest, ancestors);
    ancestors.pop();
    result
}

fn merge(
    linker: &mut Linker<'_>,
    source: &Path,
    dest: &Path,
    ancestors: &mut Vec<PathBuf>,
) -> io::Result<()> {
    let mut entries = HashMap::new();
    for entry in fs::read_dir(dest)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().to_lowercase();
        entries.insert(name, entry.path());
    }
    for item in fs::read_dir(source)? {
        let item = item?;
        let name = item.file_name();
        let item = item.path();
        let found = entries
            .get(&name.to_string_lossy().to_lowercase())
            .cloned()
            .unwrap_or_else(|| dest.join(&name));
        if item.is_dir() && found.is_dir() {
            if linker.is_link(&found) {
                // A linked folder of the base: replace the link by a real folder of links.
                let inner = linker.target(&found).ok_or_else(|| {
                    io::Error::new(
                        io::ErrorKind::NotFound,
                        format!("{}: the link leads nowhere", found.display()),
                    )
                })?;
                linker.remove(&found)?;
                fs::create_dir(&found)?;
                linker.link_entries(&found, &inner)?;
            }
            overlay_into(linker, &item, &found, ancestors)?;
        } else {
            if fs::symlink_metadata(&found).is_ok() {
                linker.remove(&found)?;
            }
            linker.make(&found, &item)?;
        }
    }
    Ok(())
}

/// Removes a file, link or folder of a view without following links into the dumps.
pub fn remove(path: &Path) -> io::Result<()> {
    remove_tree(path)
}

/// The folder holding `base` with `update` over it, made in `out` as `game-update-XXXXXX`; the
/// caller removes it with `remove_view`. The two must be the same title (the `TITLE_ID` of their
/// `sce_sys/param.sfo`), in separate folders, and the update must hold an eboot.bin.
pub fn create_view(base: &Path, update: &Path, out: &Path) -> Result<PathBuf, UpdateViewError> {
    create_view_with(base, update, out, &game_title_id, &every_way)
}

/// `create_view` reading the title IDs with `title_id` and linking only the ways `allowed` gives.
pub(crate) fn create_view_with(
    base: &Path,
    update: &Path,
    out: &Path,
    title_id: &dyn Fn(&Path) -> Option<String>,
    allowed: &dyn Fn(LinkKind) -> bool,
) -> Result<PathBuf, UpdateViewError> {
    let (base, update) = (resolve(base)?, resolve(update)?);
    let (base_id, update_id) = (title_id(&base), title_id(&update));
    if base_id.as_deref().is_none_or(str::is_empty) || base_id != update_id {
        let shown = |id: Option<String>| id.unwrap_or_else(|| "None".to_string());
        return Err(UpdateViewError::DifferentTitles {
            base: shown(base_id),
            update: shown(update_id),
        });
    }
    if base.starts_with(&update) || update.starts_with(&base) {
        return Err(UpdateViewError::NestedFolders);
    }
    if !update.join("eboot.bin").is_file() {
        return Err(UpdateViewError::NoEboot);
    }
    fs::create_dir_all(out)?;
    let target = tempfile::Builder::new()
        .prefix(VIEW_PREFIX)
        .tempdir_in(out)?
        .keep();
    let mut linker = Linker::new(allowed);
    let made = linker
        .link_entries(&target, &base)
        .and_then(|()| overlay_into(&mut linker, &update, &target, &mut Vec::new()));
    if let Err(error) = made {
        // The links go, never what they lead to; the error reported is the one that stopped the
        // view.
        let _ = remove_view(&target);
        return Err(error.into());
    }
    Ok(target)
}

/// Deletes a view made by `create_view` (the dumps stay untouched); a view already gone is fine.
pub fn remove_view(path: &Path) -> io::Result<()> {
    if fs::symlink_metadata(path).is_ok() {
        remove(path)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::mods::{is_link, LinkKind};
    use std::cell::Cell;
    use std::fs;

    fn write(path: &Path, content: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    fn read(path: &Path) -> String {
        fs::read_to_string(path).unwrap()
    }

    /// A base game and its update, as `GameUpdateTests.setUp` makes them.
    struct Dumps {
        _temp: tempfile::TempDir,
        base: PathBuf,
        update: PathBuf,
        out: PathBuf,
    }

    impl Dumps {
        fn new() -> Dumps {
            let temp = tempfile::tempdir().unwrap();
            let root = temp.path();
            let (base, update) = (root.join("base"), root.join("update"));
            write(&base.join("eboot.bin"), "base eboot");
            write(&base.join("EXEC/WAD/A.WAD"), "wad");
            write(&base.join("sce_sys/icon0.png"), "old icon");
            write(&update.join("eboot.bin"), "update eboot");
            write(&update.join("SCE_SYS/param.sfo"), "new sfo");
            write(&update.join("SCE_SYS/icon0.png"), "new icon");
            Dumps {
                out: root.join("out"),
                _temp: temp,
                base,
                update,
            }
        }

        /// The view with the title IDs given in call order (the base first), as the Python's
        /// mock of `game_title_id` gives them.
        fn view_with(
            &self,
            update: &Path,
            ids: [&str; 2],
            allowed: fn(LinkKind) -> bool,
        ) -> Result<PathBuf, UpdateViewError> {
            let calls = Cell::new(0);
            let title_id = |_: &Path| {
                let call = calls.get();
                calls.set(call + 1);
                ids.get(call).map(|id| (*id).to_string())
            };
            create_view_with(&self.base, update, &self.out, &title_id, &allowed)
        }

        fn view(&self) -> PathBuf {
            let same = ["CUSA01623", "CUSA01623"];
            self.view_with(&self.update, same, |_| true).unwrap()
        }
    }

    #[test]
    fn update_files_replace_base_files_and_the_rest_stays() {
        let d = Dumps::new();
        let view = d.view();
        assert_eq!(read(&view.join("eboot.bin")), "update eboot");
        assert_eq!(read(&view.join("EXEC/WAD/A.WAD")), "wad");
        assert_eq!(read(&view.join("sce_sys/icon0.png")), "new icon");
        assert_eq!(read(&view.join("sce_sys/param.sfo")), "new sfo");
        let system: Vec<String> = fs::read_dir(&view)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.to_lowercase() == "sce_sys")
            .collect();
        assert_eq!(system, ["sce_sys"]);
        let name = view.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with("game-update-"), "{name}");
        assert_eq!(view.parent(), Some(d.out.as_path()));
    }

    #[test]
    fn folders_the_update_leaves_alone_are_linked_whole() {
        let d = Dumps::new();
        let view = d.view();
        assert!(is_link(&view.join("EXEC")));
        assert!(!is_link(&view.join("sce_sys"))); // merged: base and update files together
    }

    #[test]
    fn removing_the_view_leaves_the_dumps_whole() {
        let d = Dumps::new();
        let view = d.view();
        remove_view(&view).unwrap();
        assert!(!view.exists());
        assert_eq!(read(&d.base.join("EXEC/WAD/A.WAD")), "wad");
        assert_eq!(read(&d.base.join("sce_sys/icon0.png")), "old icon");
        assert_eq!(read(&d.update.join("SCE_SYS/param.sfo")), "new sfo");
        remove_view(&view).unwrap(); // a view already gone is fine
    }

    #[test]
    fn dumps_are_not_modified() {
        let d = Dumps::new();
        d.view();
        assert_eq!(read(&d.base.join("eboot.bin")), "base eboot");
        assert_eq!(read(&d.base.join("sce_sys/icon0.png")), "old icon");
    }

    #[test]
    fn a_network_share_without_junctions_mirrors_the_tree() {
        // A network share (SMB) refuses symlinks and junctions alike; only hard links work.
        // create_view must still build a view of real files and leave the dumps untouched.
        let d = Dumps::new();
        let share = |kind| matches!(kind, LinkKind::HardLink | LinkKind::Mirror);
        let view = d
            .view_with(&d.update, ["CUSA01623", "CUSA01623"], share)
            .unwrap();
        assert_eq!(read(&view.join("eboot.bin")), "update eboot");
        assert_eq!(read(&view.join("EXEC/WAD/A.WAD")), "wad"); // mirrored, a real file
        assert!(!is_link(&view.join("EXEC")));
        assert_eq!(read(&view.join("sce_sys/icon0.png")), "new icon"); // merged over the base
        assert_eq!(read(&view.join("sce_sys/param.sfo")), "new sfo"); // added by the update
                                                                      // The mirrored files are hard links, not copies: the dumps stay exactly as they were.
        assert_eq!(read(&d.base.join("EXEC/WAD/A.WAD")), "wad");
        assert_eq!(read(&d.base.join("sce_sys/icon0.png")), "old icon");
        remove_view(&view).unwrap();
        assert!(!view.exists());
        assert_eq!(read(&d.base.join("EXEC/WAD/A.WAD")), "wad");
        assert_eq!(read(&d.base.join("sce_sys/icon0.png")), "old icon");
    }

    #[test]
    fn different_titles_and_nested_folders_are_refused() {
        let d = Dumps::new();
        let error = d
            .view_with(&d.update, ["CUSA01623", "CUSA01715"], |_| true)
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "Game and update are not the same title: CUSA01623 / CUSA01715"
        );
        let error = d
            .view_with(&d.base.join("EXEC"), ["CUSA01623", "CUSA01623"], |_| true)
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "Choose two separate folders for the game and its update"
        );
        let left: Vec<_> = fs::read_dir(&d.out)
            .map(|it| it.map(|e| e.unwrap().path()).collect())
            .unwrap_or_default();
        assert!(left.is_empty(), "{left:?}");
    }

    #[test]
    fn a_missing_title_or_eboot_is_refused() {
        let d = Dumps::new();
        let error = d
            .view_with(&d.update, ["", ""], |_| true)
            .map(|_| ())
            .unwrap_err();
        assert!(matches!(error, UpdateViewError::DifferentTitles { .. }));
        let none = |_: &Path| None;
        let any: fn(LinkKind) -> bool = |_| true;
        let error = create_view_with(&d.base, &d.update, &d.out, &none, &any).unwrap_err();
        assert_eq!(
            error.to_string(),
            "Game and update are not the same title: None / None"
        );
        fs::remove_file(d.update.join("eboot.bin")).unwrap();
        let error = d
            .view_with(&d.update, ["CUSA01623", "CUSA01623"], |_| true)
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "The update folder needs eboot.bin and sce_sys/param.sfo"
        );
        assert!(!d.out.exists());
    }
}

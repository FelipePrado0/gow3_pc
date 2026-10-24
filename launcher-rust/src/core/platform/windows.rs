//! Windows calls.

use std::fs;
use std::io;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use windows_sys::core::BOOL;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0};
use windows_sys::Win32::Globalization::GetUserDefaultUILanguage;
use windows_sys::Win32::Storage::FileSystem::{
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, MAXIMUM_REPARSE_DATA_BUFFER_SIZE,
};
use windows_sys::Win32::System::Console::{
    AttachConsole, GetConsoleWindow, GetStdHandle, SetConsoleCtrlHandler, SetStdHandle,
    ATTACH_PARENT_PROCESS, CTRL_BREAK_EVENT, CTRL_C_EVENT, STD_ERROR_HANDLE, STD_OUTPUT_HANDLE,
};
use windows_sys::Win32::System::Ioctl::FSCTL_SET_REPARSE_POINT;
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, TerminateJobObject,
};
use windows_sys::Win32::System::Pipes::{
    PeekNamedPipe, SetNamedPipeHandleState, NAMED_PIPE_MODE, PIPE_NOWAIT, PIPE_READMODE_BYTE,
};
use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};
use windows_sys::Win32::System::SystemServices::IO_REPARSE_TAG_MOUNT_POINT;
use windows_sys::Win32::System::Threading::{
    OpenProcess, WaitForSingleObject, CREATE_NO_WINDOW, PROCESS_SYNCHRONIZE,
};
use windows_sys::Win32::System::IO::DeviceIoControl;
use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR};

/// The user's UI language id (LANGID), or `None` when Windows reports 0.
pub fn user_default_ui_language() -> Option<u32> {
    // SAFETY: the function takes no arguments and no pointers, and always succeeds.
    let id = unsafe { GetUserDefaultUILanguage() };
    (id != 0).then_some(u32::from(id))
}

/// Steam's install folder from `HKCU\Software\Valve\Steam\SteamPath`, if the key is there.
pub fn steam_path_from_registry() -> Option<std::path::PathBuf> {
    let wide = |text: &str| -> Vec<u16> { text.encode_utf16().chain([0]).collect() };
    let (key, value) = (wide("Software\\Valve\\Steam"), wide("SteamPath"));
    let mut size: u32 = 0;
    // SAFETY: both names are NUL-terminated and outlive the call; a null data pointer only asks
    // for the size in bytes.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            &mut size,
        )
    };
    if status != 0 || size == 0 {
        return None;
    }
    let mut buf = vec![0u16; (size as usize).div_ceil(2)];
    let mut size = (buf.len() * 2) as u32;
    // SAFETY: `buf` is writable for `size` bytes and the names are as above; the call writes at
    // most `size` bytes and updates it to the length written.
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buf.as_mut_ptr().cast(),
            &mut size,
        )
    };
    if status != 0 {
        return None;
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    (len > 0).then(|| std::path::PathBuf::from(std::ffi::OsString::from_wide(&buf[..len])))
}

/// Gives a console program no console window (the launcher, a windowed program, has none to share).
pub fn no_window(cmd: &mut Command) {
    cmd.creation_flags(CREATE_NO_WINDOW);
}

/// Hides the console window of the child.
pub fn prepare(cmd: &mut Command) {
    no_window(cmd);
}

/// Started from a terminal, a windowed program has no console: this attaches to the terminal's, so
/// `--version` and start-up errors print there. Nothing changes when the output already goes
/// somewhere (a pipe or a file) or when there is no terminal (a double click).
pub fn attach_parent_console() {
    // SAFETY: GetStdHandle takes a plain id and returns a handle value or null/invalid.
    let out = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
    if !out.is_null() && out != INVALID_HANDLE_VALUE {
        return;
    }
    // SAFETY: takes a plain process id; without a parent console it fails and changes nothing.
    unsafe { AttachConsole(ATTACH_PARENT_PROCESS) };
}

/// Whether this process has a console window (the Python's `GetConsoleWindow()`): started from a
/// terminal or attached to its console, not from the launcher, Steam or a shortcut.
pub fn has_console() -> bool {
    // SAFETY: takes no arguments; returns null when the process has no console.
    !unsafe { GetConsoleWindow() }.is_null()
}

/// The Python's `no_console()`: a process without a console starts console programs without a
/// window of their own; with a console they share it.
pub fn no_console_window(cmd: &mut Command) {
    if !has_console() {
        no_window(cmd);
    }
}

/// An error dialog (`MessageBoxW`, `MB_ICONERROR`) for a program nobody watches the output of.
pub fn show_error(title: &str, text: &str) {
    let wide = |text: &str| -> Vec<u16> { text.encode_utf16().chain([0]).collect() };
    let (title, text) = (wide(title), wide(text));
    // SAFETY: both strings are NUL-terminated and outlive the call; there is no owner window.
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            title.as_ptr(),
            MB_ICONERROR,
        )
    };
}

/// Set once Ctrl+C or Ctrl+Break reached the console after `catch_interrupt`.
static INTERRUPTED: AtomicBool = AtomicBool::new(false);

unsafe extern "system" fn on_console_control(event: u32) -> BOOL {
    if event == CTRL_C_EVENT || event == CTRL_BREAK_EVENT {
        INTERRUPTED.store(true, Ordering::SeqCst);
        1
    } else {
        0
    }
}

/// From now on Ctrl+C and Ctrl+Break in the console end nothing: they set [`interrupted`], which
/// the caller checks to end in order (the Python's `KeyboardInterrupt`). The programs it starts
/// still get them as before.
pub fn catch_interrupt() {
    // SAFETY: the handler is a plain function, valid for the life of the process, that only
    // stores to an atomic.
    unsafe { SetConsoleCtrlHandler(Some(on_console_control), 1) };
}

/// Whether Ctrl+C came since `catch_interrupt`.
pub fn interrupted() -> bool {
    INTERRUPTED.load(Ordering::SeqCst)
}

/// A job object holding the game and its descendants.
pub struct Job {
    handle: HANDLE,
}

// SAFETY: a job object handle is a plain kernel handle usable from any thread.
unsafe impl Send for Job {}

impl Job {
    /// Puts `child` in a new job object. A job that could not be created is tolerated: the
    /// result then does nothing.
    pub fn attach(child: &Child) -> Job {
        // SAFETY: both arguments may be null (no security attributes, no name).
        let handle = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
        if !handle.is_null() {
            // SAFETY: `handle` is a live job object and the child's handle is valid while
            // `child` is borrowed.
            unsafe { AssignProcessToJobObject(handle, child.as_raw_handle() as HANDLE) };
        }
        Job { handle }
    }

    /// Ends every process in the job with exit code 1.
    pub fn terminate(&mut self) {
        if !self.handle.is_null() {
            // SAFETY: the handle is a live job object owned by `self`.
            unsafe { TerminateJobObject(self.handle, 1) };
        }
    }

    pub fn close(&mut self) {
        if !self.handle.is_null() {
            // SAFETY: the handle is owned by `self` and nulled right after, so it closes once.
            unsafe { CloseHandle(self.handle) };
            self.handle = std::ptr::null_mut();
        }
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        self.close();
    }
}

/// Closes this process's stdout and stderr (the same pipe for the game), so a reader of that
/// pipe sees end of file while the process keeps running. Later writes do nothing.
pub fn close_std_streams() {
    let mut closed: Vec<HANDLE> = Vec::new();
    for which in [STD_OUTPUT_HANDLE, STD_ERROR_HANDLE] {
        // SAFETY: GetStdHandle takes a plain id and returns a handle value or null/invalid.
        let handle = unsafe { GetStdHandle(which) };
        if !handle.is_null() && handle != INVALID_HANDLE_VALUE && !closed.contains(&handle) {
            // SAFETY: the handle belongs to this process and each distinct value is closed once;
            // the std slots are cleared below so nothing uses it afterwards.
            unsafe { CloseHandle(handle) };
            closed.push(handle);
        }
        // SAFETY: setting a std slot to null is always allowed.
        unsafe { SetStdHandle(which, std::ptr::null_mut()) };
    }
}

/// Waits up to `limit` for process `pid` to end. `true` when it ended or does not exist (it
/// cannot be opened), `false` when it still runs after `limit`.
pub fn wait_for_process(pid: u32, limit: Duration) -> bool {
    // INFINITE is u32::MAX: a longer limit is cut one millisecond short of it.
    let millis = u32::try_from(limit.as_millis()).unwrap_or(u32::MAX - 1);
    // SAFETY: takes plain values; a pid that cannot be opened returns null.
    let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
    if handle.is_null() {
        return true;
    }
    // SAFETY: `handle` is a live process handle with SYNCHRONIZE access, opened above.
    let result = unsafe { WaitForSingleObject(handle, millis) };
    // SAFETY: `handle` is owned here and closed once.
    unsafe { CloseHandle(handle) };
    result == WAIT_OBJECT_0
}

/// Makes `link` a junction (an NTFS mount point) to the folder `target`, which needs no
/// privilege, as Python's `_winapi.CreateJunction` does: an empty folder, then the reparse point
/// set on it with `FSCTL_SET_REPARSE_POINT`. `link` must not exist. When the file system refuses
/// the reparse point (a network share does), the empty folder is removed again.
pub fn create_junction(link: &Path, target: &Path) -> io::Result<()> {
    let (substitute, print) = junction_names(&std::path::absolute(target)?);
    let buffer = mount_point_buffer(&substitute, &print)?;
    fs::create_dir(link)?;
    let result = set_reparse_point(link, &buffer);
    if result.is_err() {
        // The folder was made empty just above; if it cannot go, an empty folder is all that stays.
        let _ = fs::remove_dir(link);
    }
    result
}

/// Sets the reparse point `buffer` on the empty folder `folder`.
fn set_reparse_point(folder: &Path, buffer: &[u8]) -> io::Result<()> {
    let file = fs::OpenOptions::new()
        .write(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(folder)?;
    let size =
        u32::try_from(buffer.len()).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))?;
    let mut returned = 0u32;
    // SAFETY: the handle is a live folder handle opened for writing and owned by `file`, which
    // outlives the call; the input pointer and size describe `buffer`, which the call only reads;
    // there is no output buffer (null, 0); `returned` is a valid u32 to write; the handle is not
    // overlapped, so the OVERLAPPED pointer is null and the call completes before returning.
    let done = unsafe {
        DeviceIoControl(
            file.as_raw_handle() as HANDLE,
            FSCTL_SET_REPARSE_POINT,
            buffer.as_ptr().cast(),
            size,
            std::ptr::null_mut(),
            0,
            &mut returned,
            std::ptr::null_mut(),
        )
    };
    if done == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// The substitute name (the NT path the junction leads to: `\??\C:\folder`) and the print name
/// (the path `dir` shows: `C:\folder`) of a junction to the absolute path `target`, in UTF-16.
fn junction_names(target: &Path) -> (Vec<u16>, Vec<u16>) {
    let wide = |text: &str| -> Vec<u16> { text.encode_utf16().collect() };
    let path: Vec<u16> = target.as_os_str().encode_wide().collect();
    let (verbatim_share, verbatim, share) = (wide(r"\\?\UNC\"), wide(r"\\?\"), wide(r"\\"));
    let (nt, nt_share) = (wide(r"\??\"), wide(r"\??\UNC\"));
    if let Some(rest) = path.strip_prefix(verbatim_share.as_slice()) {
        ([&nt_share, rest].concat(), [&share, rest].concat())
    } else if let Some(rest) = path.strip_prefix(verbatim.as_slice()) {
        ([&nt, rest].concat(), rest.to_vec())
    } else if let Some(rest) = path.strip_prefix(share.as_slice()) {
        ([&nt_share, rest].concat(), path.clone())
    } else {
        ([&nt, path.as_slice()].concat(), path)
    }
}

/// The bytes of a `REPARSE_DATA_BUFFER` for a mount point, at the offsets of its C layout:
/// ReparseTag (u32) at 0, ReparseDataLength (u16) at 4, Reserved (u16) at 6, then the
/// MountPointReparseBuffer: SubstituteNameOffset at 8, SubstituteNameLength at 10,
/// PrintNameOffset at 12, PrintNameLength at 14 (u16 each, in bytes, relative to PathBuffer and
/// without the NULs) and PathBuffer at 16: the substitute name, a NUL, the print name, a NUL.
/// ReparseDataLength counts everything after the 8-byte header. Little endian throughout.
fn mount_point_buffer(substitute: &[u16], print: &[u16]) -> io::Result<Vec<u8>> {
    const TAG: usize = 0;
    const DATA_LENGTH: usize = 4;
    const SUBSTITUTE_OFFSET: usize = 8;
    const SUBSTITUTE_LENGTH: usize = 10;
    const PRINT_OFFSET: usize = 12;
    const PRINT_LENGTH: usize = 14;
    const PATH_BUFFER: usize = 16;
    const HEADER: usize = 8;
    let too_long = || {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "the junction's target path is too long",
        )
    };
    let substitute_bytes = substitute.len() * 2;
    let print_at = substitute_bytes + 2;
    let print_bytes = print.len() * 2;
    let total = PATH_BUFFER + print_at + print_bytes + 2;
    if total > MAXIMUM_REPARSE_DATA_BUFFER_SIZE as usize {
        return Err(too_long());
    }
    let mut buffer = vec![0u8; total];
    buffer[TAG..TAG + 4].copy_from_slice(&IO_REPARSE_TAG_MOUNT_POINT.to_le_bytes());
    for (at, value) in [
        (DATA_LENGTH, total - HEADER),
        (SUBSTITUTE_OFFSET, 0),
        (SUBSTITUTE_LENGTH, substitute_bytes),
        (PRINT_OFFSET, print_at),
        (PRINT_LENGTH, print_bytes),
    ] {
        let value = u16::try_from(value).map_err(|_| too_long())?;
        buffer[at..at + 2].copy_from_slice(&value.to_le_bytes());
    }
    // Reserved and the two NULs stay zero.
    let substitute_units = substitute
        .iter()
        .enumerate()
        .map(|(i, u)| (PATH_BUFFER + 2 * i, u));
    let print_units = print
        .iter()
        .enumerate()
        .map(|(i, u)| (PATH_BUFFER + print_at + 2 * i, u));
    for (at, unit) in substitute_units.chain(print_units) {
        buffer[at..at + 2].copy_from_slice(&unit.to_le_bytes());
    }
    Ok(buffer)
}

/// Puts the client end of a named pipe in non-blocking byte mode (`PIPE_NOWAIT`), as the Python
/// Discord presence does: a read or a write then returns at once with what the pipe holds or can
/// take, so a silent or stalled server never holds up the caller.
pub fn set_pipe_nowait(pipe: &fs::File) -> io::Result<()> {
    let mode: NAMED_PIPE_MODE = PIPE_NOWAIT | PIPE_READMODE_BYTE;
    // SAFETY: the handle is the live pipe handle owned by `pipe`, which outlives the call; `mode`
    // is a valid value to read; the collection count and timeout are not changed (null).
    let done = unsafe {
        SetNamedPipeHandleState(
            pipe.as_raw_handle() as HANDLE,
            &mode,
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    if done == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// The bytes waiting to be read from a named pipe, without reading them (`PeekNamedPipe`); an
/// error once the other end is gone.
pub fn pipe_bytes_available(pipe: &fs::File) -> io::Result<u32> {
    let mut available = 0u32;
    // SAFETY: the handle is the live pipe handle owned by `pipe`, which outlives the call; there
    // is no buffer to copy into (null, 0); `available` is a valid u32 to write and the other two
    // out pointers are null.
    let done = unsafe {
        PeekNamedPipe(
            pipe.as_raw_handle() as HANDLE,
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            &mut available,
            std::ptr::null_mut(),
        )
    };
    if done == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(available)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::fs::MetadataExt;

    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

    #[test]
    fn a_junction_leads_to_its_target_and_goes_without_it() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("target folder");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("file.txt"), b"inside").unwrap();
        let link = temp.path().join("link");
        create_junction(&link, &target).unwrap();
        let meta = fs::symlink_metadata(&link).unwrap();
        assert_ne!(meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT, 0);
        assert_eq!(fs::read(link.join("file.txt")).unwrap(), b"inside");
        assert_eq!(
            fs::canonicalize(&link).unwrap(),
            fs::canonicalize(&target).unwrap()
        );
        fs::remove_dir(&link).unwrap();
        assert_eq!(fs::read(target.join("file.txt")).unwrap(), b"inside");
    }

    #[test]
    fn a_junction_over_an_existing_entry_is_refused_and_changes_nothing() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("target");
        fs::create_dir(&target).unwrap();
        let link = temp.path().join("link");
        fs::create_dir(&link).unwrap();
        fs::write(link.join("mine.txt"), b"mine").unwrap();
        assert!(create_junction(&link, &target).is_err());
        assert_eq!(fs::read(link.join("mine.txt")).unwrap(), b"mine");
        let meta = fs::symlink_metadata(&link).unwrap();
        assert_eq!(meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT, 0);
    }

    #[test]
    fn the_mount_point_buffer_is_laid_out_as_the_reparse_data_buffer() {
        let (substitute, print) = junction_names(Path::new(r"C:\t"));
        let wide = |text: &str| text.encode_utf16().collect::<Vec<u16>>();
        assert_eq!(substitute, wide(r"\??\C:\t"));
        assert_eq!(print, wide(r"C:\t"));
        let buffer = mount_point_buffer(&substitute, &print).unwrap();
        let u16_at = |at: usize| u16::from_le_bytes([buffer[at], buffer[at + 1]]);
        // 8-byte header + 8 bytes of offsets and lengths + "\??\C:\t" NUL "C:\t" NUL.
        assert_eq!(buffer.len(), 8 + 8 + 2 * (8 + 1 + 4 + 1));
        assert_eq!(&buffer[0..4], &0xA000_0003u32.to_le_bytes());
        assert_eq!(usize::from(u16_at(4)), buffer.len() - 8); // ReparseDataLength
        assert_eq!(u16_at(6), 0); // Reserved
        assert_eq!((u16_at(8), u16_at(10)), (0, 16)); // the substitute name
        assert_eq!((u16_at(12), u16_at(14)), (18, 8)); // the print name, after a NUL
        let names: Vec<u16> = buffer[16..]
            .chunks(2)
            .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
            .collect();
        let mut expected = wide(r"\??\C:\t");
        expected.push(0);
        expected.extend(wide(r"C:\t"));
        expected.push(0);
        assert_eq!(names, expected);
    }

    #[test]
    fn junction_names_of_verbatim_and_share_paths() {
        let wide = |text: &str| text.encode_utf16().collect::<Vec<u16>>();
        let names = |path: &str| junction_names(Path::new(path));
        assert_eq!(
            names(r"\\?\C:\games\x"),
            (wide(r"\??\C:\games\x"), wide(r"C:\games\x"))
        );
        assert_eq!(
            names(r"\\?\UNC\server\share\x"),
            (wide(r"\??\UNC\server\share\x"), wide(r"\\server\share\x"))
        );
        assert_eq!(
            names(r"\\server\share\x"),
            (wide(r"\??\UNC\server\share\x"), wide(r"\\server\share\x"))
        );
    }

    #[test]
    fn a_target_too_long_for_a_reparse_point_is_refused() {
        let long = vec![u16::from(b'a'); 9000];
        assert!(mount_point_buffer(&long, &long).is_err());
        // Refused before the folder is made.
        let temp = tempfile::tempdir().unwrap();
        let link = temp.path().join("link");
        let target = temp.path().join("a".repeat(9000));
        assert!(create_junction(&link, &target).is_err());
        assert!(fs::symlink_metadata(&link).is_err());
    }
}

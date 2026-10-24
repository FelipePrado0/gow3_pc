//! Memory check for the guest memory pool (Python `scripts/guest_memory.py`).

/// `runtime_memory.c` `pool_size_bytes` default.
pub const DMEM_RETAIL_MB: u64 = 5056;
/// `runtime_memory.c` `FLEX_SPAN`.
pub const FLEX_SPAN_MB: u64 = 1024;
/// The texture fix (on by default) and the 480p/720p patches ask for DMEM + 2048 MB: games that
/// run at those sizes are not warned, only the resolution patches above 1080p.
pub const DMEM_DEFAULT_MB: u64 = DMEM_RETAIL_MB + 2048;

/// The probe's own memory besides the pool, by where the stream buffers live. Measured on one PC
/// (RTX 5070, 16 GB, 4K): about 2 GiB with them in GPU memory, more in system RAM.
fn headroom_mb(stream_memory: &str) -> u64 {
    match stream_memory {
        "device" => 2048,
        _ => 4096, // "cached", "host" and anything unknown
    }
}

/// The section `runtime_memory.c` commits for `SPARTAN_DMEM_MB` (0 or out of range: the retail size).
pub fn pool_mb(dmem_mb: u64) -> u64 {
    let mb = if (DMEM_RETAIL_MB..=16384).contains(&dmem_mb) {
        dmem_mb
    } else {
        DMEM_RETAIL_MB
    };
    mb + FLEX_SPAN_MB
}

/// The installed RAM in MiB (`ullTotalPhys`), or `None` when the system does not say.
#[cfg(windows)]
pub fn physical_memory_mb() -> Option<u64> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

    // SAFETY: an all-zero MEMORYSTATUSEX is a valid value (plain integers only).
    let mut status: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    status.dwLength = u32::try_from(std::mem::size_of::<MEMORYSTATUSEX>()).ok()?;
    // SAFETY: `status` is a live MEMORYSTATUSEX whose dwLength is set, as the call requires.
    let ok = unsafe { GlobalMemoryStatusEx(&mut status) };
    (ok != 0).then_some(status.ullTotalPhys >> 20)
}

/// Off Windows the installed RAM is not read (the Python returns `None` too): the warning's advice
/// is about the Windows page file and error 1450.
#[cfg(not(windows))]
pub fn physical_memory_mb() -> Option<u64> {
    None
}

/// A warning when the pool and the probe's headroom need more than the installed RAM.
pub fn memory_warning(
    dmem_mb: u64,
    physical_mb: Option<u64>,
    stream_memory: &str,
) -> Option<String> {
    let pool = pool_mb(dmem_mb);
    let physical = physical_mb?;
    if pool <= DMEM_DEFAULT_MB + FLEX_SPAN_MB {
        return None;
    }
    let needed = pool + headroom_mb(stream_memory);
    if needed <= physical {
        return None;
    }
    let advice = if stream_memory == "device" {
        ""
    } else {
        "set Performance -> Stream buffer memory to GPU memory, "
    };
    Some(format!(
        "Memory: the guest memory pool commits {pool} MiB and the game needs about {needed} MiB in all, more than the {physical} MiB of RAM. The game may close at start (error 1450): {advice}make sure the Windows page file can grow, or choose a lower render resolution."
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const RAM_16_GB: u64 = 16303;

    #[test]
    fn pool_matches_runtime_memory() {
        assert_eq!(pool_mb(0), 5056 + 1024);
        assert_eq!(pool_mb(11200), 11200 + 1024);
        assert_eq!(pool_mb(4000), 5056 + 1024);
        assert_eq!(pool_mb(20000), 5056 + 1024);
    }

    #[test]
    fn reported_pc() {
        let warning = memory_warning(11200, Some(RAM_16_GB), "cached").unwrap_or_default();
        assert!(warning.contains("12224 MiB"));
        assert!(warning.contains("GPU memory"));
        assert_eq!(memory_warning(11200, Some(RAM_16_GB), "device"), None);
        assert_eq!(memory_warning(8128, Some(RAM_16_GB), "cached"), None);
        assert_eq!(memory_warning(0, Some(RAM_16_GB), "cached"), None);
    }

    #[test]
    fn device_advice_is_not_repeated() {
        let warning = memory_warning(11200, Some(12000), "device").unwrap_or_default();
        assert!(!warning.contains("GPU memory"));
        assert!(warning.contains("page file"));
    }

    #[test]
    fn enough_ram_is_silent() {
        assert_eq!(memory_warning(11200, Some(32000), "host"), None);
    }

    #[test]
    fn default_patches_are_silent_on_8_gb() {
        assert_eq!(memory_warning(0, Some(8000), "cached"), None);
        assert_eq!(memory_warning(7104, Some(8000), "cached"), None);
    }

    #[test]
    fn warns_for_1440p_on_8_gb() {
        let warning = memory_warning(8128, Some(8000), "cached").unwrap_or_default();
        assert!(warning.contains("9152 MiB"));
    }

    #[test]
    fn warning_text_is_the_pythons() {
        // `guest_memory.memory_warning(8128, 8000, 'cached')` of scripts/guest_memory.py.
        let python = "Memory: the guest memory pool commits 9152 MiB and the game needs about 13248 MiB in all, more than the 8000 MiB of RAM. The game may close at start (error 1450): set Performance -> Stream buffer memory to GPU memory, make sure the Windows page file can grow, or choose a lower render resolution.";
        assert_eq!(
            memory_warning(8128, Some(8000), "cached").as_deref(),
            Some(python)
        );
    }

    #[cfg(windows)]
    #[test]
    fn physical_memory_is_reported() {
        assert!(physical_memory_mb().is_some_and(|mb| mb > 0));
    }

    #[cfg(not(windows))]
    #[test]
    fn physical_memory_is_unknown_off_windows() {
        assert_eq!(physical_memory_mb(), None);
    }

    #[test]
    fn unknown_ram_is_silent() {
        assert_eq!(memory_warning(11200, None, "cached"), None);
    }

    #[test]
    fn unknown_stream_memory_counts_as_cached() {
        assert_eq!(
            memory_warning(11200, Some(RAM_16_GB), "other"),
            memory_warning(11200, Some(RAM_16_GB), "cached")
        );
    }
}

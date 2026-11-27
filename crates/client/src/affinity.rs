//! Pins the calling thread to a performance core, so one-worker timings on hybrid CPUs do not depend on where the
//! OS placed the thread (ADR 0080).

use civbb_format::cost::Pinned;

/// Pins the calling thread to the highest-numbered logical CPU of the fastest core class (Windows: the highest
/// `EfficiencyClass` in processor group 0). `None` where that is not supported (every other OS) or the OS refuses;
/// the caller then runs unpinned. `ran_on` is the CPU the thread is on right after pinning.
pub fn pin_to_performance_core() -> Option<Pinned> {
    imp::pin()
}

/// Keeps the calling thread on the fastest core class (all of its logical CPUs in processor group 0), for worker
/// threads on hybrid CPUs (design doc §5, ADR 0103). `false` where not supported or the OS refuses; the thread then
/// runs anywhere.
pub fn keep_to_performance_cores() -> bool {
    imp::keep()
}

/// The number of logical CPUs in the fastest core class (processor group 0); `None` where not supported.
pub fn performance_cpus() -> Option<usize> {
    imp::performance_cpus()
}

/// The logical CPU the calling thread is running on; `None` where not supported.
pub fn current_cpu() -> Option<u32> {
    imp::current()
}

#[cfg(windows)]
mod imp {
    use civbb_format::cost::Pinned;
    use windows_sys::Win32::System::SystemInformation::{
        CpuSetInformation, GetSystemCpuSetInformation, SYSTEM_CPU_SET_INFORMATION,
    };
    use windows_sys::Win32::System::Threading::{GetCurrentProcessorNumber, GetCurrentThread, SetThreadAffinityMask};

    /// `(logical CPU in group 0, efficiency class)` of every CPU set the system reports.
    fn cpus() -> Option<Vec<(u32, u8)>> {
        let mut len = 0u32;
        // SAFETY: a null buffer of length 0 only asks for the needed length, written to `len`; null process means
        // the system's sets.
        unsafe { GetSystemCpuSetInformation(std::ptr::null_mut(), 0, &mut len, std::ptr::null_mut(), 0) };
        // u64 words: the entries hold a u64 and must be 8-byte aligned.
        let mut buf = vec![0u64; (len as usize).div_ceil(8)];
        // SAFETY: `buf` is writable for `len` bytes (rounded up to whole words) and 8-byte aligned.
        let ok = unsafe { GetSystemCpuSetInformation(buf.as_mut_ptr().cast(), len, &mut len, std::ptr::null_mut(), 0) };
        if ok == 0 {
            return None;
        }
        let bytes = len as usize;
        let mut out = Vec::new();
        let mut at = 0;
        while at + size_of::<SYSTEM_CPU_SET_INFORMATION>() <= bytes {
            // SAFETY: `at + size_of` is within the `bytes` the call wrote to `buf`; read_unaligned allows any offset
            // and the struct is plain data.
            let info: SYSTEM_CPU_SET_INFORMATION =
                unsafe { std::ptr::read_unaligned(buf.as_ptr().cast::<u8>().add(at).cast()) };
            if info.Size == 0 {
                break;
            }
            if info.Type == CpuSetInformation {
                // SAFETY: `Type == CpuSetInformation` says the union holds `CpuSet`.
                let set = unsafe { info.Anonymous.CpuSet };
                if set.Group == 0 {
                    out.push((u32::from(set.LogicalProcessorIndex), set.EfficiencyClass));
                }
            }
            at += info.Size as usize;
        }
        Some(out)
    }

    pub fn pin() -> Option<Pinned> {
        let cpus = cpus()?;
        let &(logical_cpu, efficiency_class) = cpus.iter().max_by_key(|&&(cpu, class)| (class, cpu))?;
        if logical_cpu >= usize::BITS {
            return None;
        }
        // SAFETY: GetCurrentThread returns a pseudo-handle valid for the calling thread; the mask names one CPU that
        // exists in the thread's group.
        let previous = unsafe { SetThreadAffinityMask(GetCurrentThread(), 1usize << logical_cpu) };
        if previous == 0 {
            return None;
        }
        Some(Pinned {
            logical_cpu,
            efficiency_class,
            ran_on: current()?,
        })
    }

    pub fn performance_cpus() -> Option<usize> {
        let cpus = cpus()?;
        let fastest = cpus.iter().map(|&(_, class)| class).max()?;
        Some(cpus.iter().filter(|&&(_, class)| class == fastest).count())
    }

    pub fn keep() -> bool {
        let Some(cpus) = cpus() else { return false };
        let Some(fastest) = cpus.iter().map(|&(_, class)| class).max() else {
            return false;
        };
        let mask = cpus
            .iter()
            .filter(|&&(cpu, class)| class == fastest && cpu < usize::BITS)
            .fold(0usize, |m, &(cpu, _)| m | 1 << cpu);
        // SAFETY: GetCurrentThread returns a pseudo-handle valid for the calling thread; the mask names only CPUs
        // that exist in the thread's group, and at least one (the fastest class is someone's).
        mask != 0 && unsafe { SetThreadAffinityMask(GetCurrentThread(), mask) } != 0
    }

    pub fn current() -> Option<u32> {
        // SAFETY: no arguments, no preconditions.
        Some(unsafe { GetCurrentProcessorNumber() })
    }
}

#[cfg(not(windows))]
mod imp {
    use civbb_format::cost::Pinned;

    pub fn pin() -> Option<Pinned> {
        None
    }

    pub fn performance_cpus() -> Option<usize> {
        None
    }

    pub fn keep() -> bool {
        false
    }

    pub fn current() -> Option<u32> {
        None
    }
}

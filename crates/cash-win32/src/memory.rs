//! The machine's memory, as `free` reports it: physical memory, the system cache, the
//! commit charge, and the page files that stand for swap.
//!
//! `GetPerformanceInfo` gives the physical and commit counts in one call; the page files
//! come from `NtQuerySystemInformation(SystemPagefileInformation)`, which is the call
//! Task Manager's and `wmic pagefile`'s numbers come from, one record per page file.

use windows_sys::Win32::System::ProcessStatus::{K32GetPerformanceInfo, PERFORMANCE_INFORMATION};

use crate::sysinfo::{INFO_LENGTH_MISMATCH, nt_query_system_information};

/// Memory counts in bytes, from one `GetPerformanceInfo` call.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MemoryCounts {
    /// The page size the counts were given in.
    pub page_size: u64,
    /// Installed usable physical memory.
    pub physical_total: u64,
    /// Physical memory Windows can hand out at once: free, zeroed and standby pages.
    pub physical_available: u64,
    /// The system cache: the standby list plus the system's own working set.
    pub system_cache: u64,
    /// Bytes committed by every process and the system.
    pub commit_total: u64,
    /// What the commit charge may grow to before the page files do.
    pub commit_limit: u64,
    /// Kernel memory in the paged pool.
    pub kernel_paged: u64,
    /// Kernel memory in the nonpaged pool.
    pub kernel_nonpaged: u64,
}

/// The page files together, in bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PageFiles {
    /// Their sizes added up; 0 without a page file.
    pub total: u64,
    /// The part of them in use.
    pub used: u64,
}

/// The counts now; `None` when Windows will not say.
#[must_use]
pub fn counts() -> Option<MemoryCounts> {
    let mut info = PERFORMANCE_INFORMATION {
        cb: u32::try_from(size_of::<PERFORMANCE_INFORMATION>()).unwrap_or(u32::MAX),
        ..Default::default()
    };
    // SAFETY: `info` is correctly sized and its size is passed alongside the pointer.
    if unsafe { K32GetPerformanceInfo(&raw mut info, info.cb) } == 0 {
        return None;
    }
    let page_size = u64::try_from(info.PageSize).unwrap_or(u64::MAX);
    let bytes = |pages: usize| {
        u64::try_from(pages)
            .unwrap_or(u64::MAX)
            .saturating_mul(page_size)
    };
    Some(MemoryCounts {
        page_size,
        physical_total: bytes(info.PhysicalTotal),
        physical_available: bytes(info.PhysicalAvailable),
        system_cache: bytes(info.SystemCache),
        commit_total: bytes(info.CommitTotal),
        commit_limit: bytes(info.CommitLimit),
        kernel_paged: bytes(info.KernelPaged),
        kernel_nonpaged: bytes(info.KernelNonpaged),
    })
}

/// `SystemPagefileInformation`.
const PAGEFILE_INFORMATION: i32 = 18;

/// One record of `SystemPagefileInformation`: the counts are in pages, and the name,
/// which follows the record, is not read.
#[repr(C)]
#[derive(Clone, Copy, Default)]
struct PagefileRecord {
    next_entry_offset: u32,
    total_size: u32,
    total_in_use: u32,
    peak_usage: u32,
    name: windows_sys::Win32::Foundation::UNICODE_STRING,
}

/// The page files now, their pages counted in `page_size` bytes; `None` when Windows
/// will not say, and zeros when there is no page file.
#[must_use]
pub fn page_files(page_size: u64) -> Option<PageFiles> {
    let query = nt_query_system_information()?;
    // `u64`s so that the records in it are aligned.
    let mut buffer: Vec<u64> = vec![0; 1 << 10];
    for _ in 0..4 {
        let capacity = u32::try_from(buffer.len() * 8).unwrap_or(u32::MAX);
        let mut needed = 0u32;
        // SAFETY: the buffer holds `capacity` writable bytes and `needed` is a valid
        // out-parameter.
        let status = unsafe {
            query(
                PAGEFILE_INFORMATION,
                buffer.as_mut_ptr().cast(),
                capacity,
                &raw mut needed,
            )
        };
        if status == INFO_LENGTH_MISMATCH {
            buffer.resize((needed as usize).div_ceil(8).max(buffer.len() * 2), 0);
            continue;
        }
        if status < 0 {
            return None;
        }
        let length = (needed as usize).min(buffer.len() * 8);
        // SAFETY: the call succeeded and wrote `length` bytes of page file records.
        return Some(unsafe { sum_page_files(buffer.as_ptr().cast(), length, page_size) });
    }
    None
}

/// Adds up the page file records at `base`, `length` bytes of them.
///
/// # Safety
///
/// `base` must point to `length` readable bytes holding the records the call wrote.
unsafe fn sum_page_files(base: *const u8, length: usize, page_size: u64) -> PageFiles {
    let record_size = size_of::<PagefileRecord>();
    let mut files = PageFiles::default();
    let mut offset = 0usize;
    // Without a page file the call writes nothing, and the loop ends at once.
    while offset.saturating_add(record_size) <= length {
        // SAFETY: the record lies within the buffer, checked just above.
        let record: PagefileRecord =
            unsafe { std::ptr::read_unaligned(base.wrapping_add(offset).cast()) };
        let bytes = |pages: u32| u64::from(pages).saturating_mul(page_size);
        files.total = files.total.saturating_add(bytes(record.total_size));
        files.used = files.used.saturating_add(bytes(record.total_in_use));
        if record.next_entry_offset == 0 {
            break;
        }
        offset = offset.saturating_add(record.next_entry_offset as usize);
    }
    files.used = files.used.min(files.total);
    files
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert loudly on failure"
)]
mod tests {
    use super::*;

    #[test]
    fn the_counts_describe_this_machine() {
        let counts = counts().expect("performance information");
        assert!(counts.page_size >= 4096);
        assert!(counts.physical_total > 0);
        assert!(counts.physical_available <= counts.physical_total);
        assert!(counts.commit_total <= counts.commit_limit);
        // The same numbers `top` reads through `sysinfo`.
        let status = crate::sysinfo::memory_status().expect("memory status");
        assert_eq!(counts.physical_total, status.physical_total);
    }

    #[test]
    fn the_page_files_add_up() {
        let counts = counts().expect("performance information");
        let files = page_files(counts.page_size).expect("page file information");
        assert!(files.used <= files.total, "{files:?}");
        // The commit limit is physical memory plus the page files, less a little.
        assert!(
            files.total <= counts.commit_limit,
            "{files:?} against commit limit {}",
            counts.commit_limit
        );
    }

    #[test]
    fn records_are_summed_as_windows_lays_them_out() {
        let mut buffer = vec![0u64; 64];
        let base = buffer.as_mut_ptr().cast::<u8>();
        let size = size_of::<PagefileRecord>();
        // Two records, the name of each (not read) between it and the next.
        let first = PagefileRecord {
            next_entry_offset: u32::try_from(size + 16).unwrap(),
            total_size: 1000,
            total_in_use: 100,
            ..Default::default()
        };
        let second = PagefileRecord {
            next_entry_offset: 0,
            total_size: 500,
            total_in_use: 600,
            ..Default::default()
        };
        // SAFETY: both writes land inside the 512-byte buffer.
        unsafe {
            std::ptr::write_unaligned(base.cast(), first);
        }
        // SAFETY: as above.
        unsafe {
            std::ptr::write_unaligned(base.wrapping_add(size + 16).cast(), second);
        }
        // SAFETY: the buffer holds the records just written.
        let files = unsafe { sum_page_files(base, buffer.len() * 8, 4096) };
        // In use is capped at the total: a record cannot be fuller than its file.
        assert_eq!(
            files,
            PageFiles {
                total: 1500 * 4096,
                used: 700 * 4096
            }
        );
        // SAFETY: an empty buffer, as a machine without a page file returns.
        let none = unsafe { sum_page_files(base, 0, 4096) };
        assert_eq!(none, PageFiles::default());
    }
}

//! Pointing this process's imports of a Windows function at a function of cash's: see
//! [`redirect`].
//!
//! An executable calls a DLL's function through its import table, one slot per imported
//! function, which the loader fills with the function's address. Writing another address
//! into the slot makes every call the executable makes to that function, from any crate
//! linked into it, go there instead: what Microsoft's Detours calls IAT patching. Only
//! the executable's own table is changed, so the DLLs it loads call the real function.

use std::sync::atomic::{AtomicUsize, Ordering};

use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Memory::{PAGE_PROTECTION_FLAGS, PAGE_READWRITE, VirtualProtect};
use windows_sys::Win32::System::ProcessStatus::{GetModuleInformation, MODULEINFO};
use windows_sys::Win32::System::Threading::GetCurrentProcess;

/// Offsets into a PE32+ image, from the PE format's specification.
const NT_HEADERS_OFFSET: usize = 0x3C;
const PE_SIGNATURE: u32 = 0x0000_4550;
const OPTIONAL_HEADER: usize = 24;
const PE32_PLUS: u16 = 0x020B;
const IMPORT_DIRECTORY: usize = 112 + 8;
const DESCRIPTOR_SIZE: usize = 20;
const ORDINAL_FLAG: u64 = 1 << 63;

/// Points every import of the function `name` in this process's executable at `target`,
/// and keeps the address the first one held in `original`, for `target` to call on.
///
/// Returns whether one was found. To be called before any thread that may call the
/// function is started.
pub(crate) fn redirect(name: &str, target: usize, original: &AtomicUsize) -> bool {
    // SAFETY: a null name asks for the executable's own module, which is mapped for the
    // life of the process.
    let module = unsafe { GetModuleHandleW(std::ptr::null()) };
    if module.is_null() {
        return false;
    }
    let mut info = MODULEINFO {
        lpBaseOfDll: std::ptr::null_mut(),
        SizeOfImage: 0,
        EntryPoint: std::ptr::null_mut(),
    };
    // SAFETY: always safe.
    let process = unsafe { GetCurrentProcess() };
    let size = u32::try_from(size_of::<MODULEINFO>()).unwrap_or(0);
    // SAFETY: `module` is this process's, and `info` a valid out-param of that size.
    if unsafe { GetModuleInformation(process, module, &raw mut info, size) } == 0 {
        return false;
    }
    let base = info.lpBaseOfDll.cast::<u8>();
    let Ok(length) = usize::try_from(info.SizeOfImage) else {
        return false;
    };

    let slots = {
        // SAFETY: the loader maps the whole image, every section readable, for the life
        // of the process. The slice is dropped before anything is written into it.
        let image = unsafe { std::slice::from_raw_parts(base.cast_const(), length) };
        import_slots(image, name).unwrap_or_default()
    };

    let mut found = false;
    for offset in slots {
        // An import slot is pointer-aligned within the image, which the loader maps at a
        // page boundary.
        #[expect(clippy::cast_ptr_alignment, reason = "import slots are aligned")]
        let slot = base.wrapping_add(offset).cast::<usize>();
        // SAFETY: `slot` is an import slot within the mapped image, found above.
        found |= unsafe { patch(slot, target, original) };
    }
    found
}

/// Where in `image`, a PE32+ image as mapped, the import slots of the function `name`
/// are; `None` if the image is not one.
fn import_slots(image: &[u8], name: &str) -> Option<Vec<usize>> {
    let nt = usize::try_from(read_u32(image, NT_HEADERS_OFFSET)?).ok()?;
    if read_u32(image, nt)? != PE_SIGNATURE {
        return None;
    }
    let optional = nt + OPTIONAL_HEADER;
    if read_u16(image, optional)? != PE32_PLUS {
        return None;
    }
    let mut descriptor = usize::try_from(read_u32(image, optional + IMPORT_DIRECTORY)?).ok()?;
    if descriptor == 0 {
        return None;
    }

    let mut slots = Vec::new();
    loop {
        let names = usize::try_from(read_u32(image, descriptor)?).ok()?;
        let dll = read_u32(image, descriptor + 12)?;
        let first_slot = usize::try_from(read_u32(image, descriptor + 16)?).ok()?;
        if dll == 0 {
            break;
        }
        let lookup = if names == 0 { first_slot } else { names };
        for index in 0.. {
            let entry = read_u64(image, lookup + index * 8)?;
            if entry == 0 {
                break;
            }
            if entry & ORDINAL_FLAG != 0 {
                continue;
            }
            // An import by name: a two-byte hint, then the name, null-terminated.
            let at = usize::try_from(entry & 0x7FFF_FFFF).ok()? + 2;
            let imported = image.get(at..)?;
            let end = imported.iter().position(|&byte| byte == 0)?;
            if imported.get(..end)? == name.as_bytes() {
                slots.push(first_slot + index * 8);
            }
        }
        descriptor += DESCRIPTOR_SIZE;
    }
    Some(slots)
}

fn read_u16(image: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(image.get(at..at + 2)?.try_into().ok()?))
}

fn read_u32(image: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(image.get(at..at + 4)?.try_into().ok()?))
}

fn read_u64(image: &[u8], at: usize) -> Option<u64> {
    Some(u64::from_le_bytes(image.get(at..at + 8)?.try_into().ok()?))
}

/// Writes `target` into the import slot `slot`, keeping what it held in `original`.
///
/// # Safety
///
/// `slot` must be an import slot of the mapped executable, and no other thread may be
/// calling through it.
unsafe fn patch(slot: *mut usize, target: usize, original: &AtomicUsize) -> bool {
    let mut protection: PAGE_PROTECTION_FLAGS = 0;
    let bytes = size_of::<usize>();
    // SAFETY: the slot is within the image; its page is made writable for the write.
    if unsafe { VirtualProtect(slot.cast(), bytes, PAGE_READWRITE, &raw mut protection) } == 0 {
        return false;
    }
    // SAFETY: the slot is readable and aligned, as the loader wrote it.
    let previous = unsafe { slot.read() };
    if previous != target {
        let _ = original.compare_exchange(0, previous, Ordering::AcqRel, Ordering::Acquire);
        // SAFETY: the page is writable now, and nothing else writes the slot.
        unsafe { slot.write(target) };
    }
    // SAFETY: the page is given its protection back.
    unsafe { VirtualProtect(slot.cast(), bytes, protection, &raw mut protection) };
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_executable_imports_what_the_devices_answer() {
        // SAFETY: as in `redirect`; nothing is written.
        let module = unsafe { GetModuleHandleW(std::ptr::null()) };
        let mut info = MODULEINFO {
            lpBaseOfDll: std::ptr::null_mut(),
            SizeOfImage: 0,
            EntryPoint: std::ptr::null_mut(),
        };
        // SAFETY: as in `redirect`.
        let process = unsafe { GetCurrentProcess() };
        let size = u32::try_from(size_of::<MODULEINFO>()).unwrap();
        // SAFETY: as in `redirect`.
        let answered = unsafe { GetModuleInformation(process, module, &raw mut info, size) };
        assert_ne!(answered, 0);
        // SAFETY: as in `redirect`.
        let image = unsafe {
            std::slice::from_raw_parts(
                info.lpBaseOfDll.cast::<u8>().cast_const(),
                usize::try_from(info.SizeOfImage).unwrap(),
            )
        };
        // Every program built with the standard library opens files through it.
        let slots = import_slots(image, "CreateFileW").unwrap();
        assert!(!slots.is_empty(), "CreateFileW is not imported");
        assert!(
            import_slots(image, "NoSuchFunctionAnywhere")
                .unwrap()
                .is_empty()
        );
        assert_eq!(import_slots(&image[..64], "CreateFileW"), None);
    }
}

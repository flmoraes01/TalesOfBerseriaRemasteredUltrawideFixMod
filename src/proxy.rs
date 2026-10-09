//! version.dll proxy: every export jumps to the real system32 version.dll.
//! The game's UnityPlayer.dll imports version.dll, so placing this DLL next to
//! the game executable gets the mod loaded at startup.

use crate::win::*;

macro_rules! proxy {
    ($($idx:literal $name:ident),* $(,)?) => {
        const NAMES: &[&str] = &[$(stringify!($name)),*];
        #[no_mangle]
        static mut ORIGINAL: [usize; NAMES.len()] = [0; NAMES.len()];
        core::arch::global_asm!(
            $(
                concat!(".globl ", stringify!($name)),
                concat!(stringify!($name), ":"),
                concat!("jmp qword ptr [rip + {tbl} + ", stringify!($idx), " * 8]"),
            )*
            tbl = sym ORIGINAL,
        );
    };
}

proxy! {
    0 GetFileVersionInfoA,
    1 GetFileVersionInfoByHandle,
    2 GetFileVersionInfoExA,
    3 GetFileVersionInfoExW,
    4 GetFileVersionInfoSizeA,
    5 GetFileVersionInfoSizeExA,
    6 GetFileVersionInfoSizeExW,
    7 GetFileVersionInfoSizeW,
    8 GetFileVersionInfoW,
    9 VerFindFileA,
    10 VerFindFileW,
    11 VerInstallFileA,
    12 VerInstallFileW,
    13 VerLanguageNameA,
    14 VerLanguageNameW,
    15 VerQueryValueA,
    16 VerQueryValueW,
}

/// Loads the real version.dll from the system directory and fills the jump table.
pub unsafe fn init() -> bool {
    let mut buf = [0u16; 260];
    let n = GetSystemDirectoryW(buf.as_mut_ptr(), buf.len() as u32) as usize;
    let mut path: Vec<u16> = buf[..n].to_vec();
    path.extend("\\version.dll\0".encode_utf16());
    let real = LoadLibraryW(path.as_ptr());
    if real.is_null() {
        return false;
    }
    let table = &mut *core::ptr::addr_of_mut!(ORIGINAL);
    for (slot, name) in table.iter_mut().zip(NAMES) {
        let cname = format!("{name}\0");
        *slot = GetProcAddress(real, cname.as_ptr()) as usize;
    }
    true
}

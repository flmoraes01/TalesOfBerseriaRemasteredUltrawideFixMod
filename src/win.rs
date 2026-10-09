//! Minimal Win32/NT bindings used by the mod (no external crates).
#![allow(non_snake_case, non_camel_case_types)]

use core::ffi::c_void;

pub type HMODULE = *mut c_void;

#[repr(C)]
pub struct UNICODE_STRING {
    pub Length: u16,
    pub MaximumLength: u16,
    pub Buffer: *const u16,
}

#[repr(C)]
pub struct LDR_DLL_NOTIFICATION_DATA {
    pub Flags: u32,
    pub FullDllName: *const UNICODE_STRING,
    pub BaseDllName: *const UNICODE_STRING,
    pub DllBase: *mut c_void,
    pub SizeOfImage: u32,
}

pub const LDR_DLL_NOTIFICATION_REASON_LOADED: u32 = 1;
pub const PAGE_EXECUTE_READWRITE: u32 = 0x40;
pub const DLL_PROCESS_ATTACH: u32 = 1;

pub type LdrDllNotification =
    unsafe extern "system" fn(reason: u32, data: *const LDR_DLL_NOTIFICATION_DATA, ctx: *mut c_void);
pub type LdrRegisterDllNotificationFn = unsafe extern "system" fn(
    flags: u32,
    cb: LdrDllNotification,
    ctx: *mut c_void,
    cookie: *mut *mut c_void,
) -> i32;

#[link(name = "kernel32")]
extern "system" {
    pub fn LoadLibraryW(name: *const u16) -> HMODULE;
    pub fn GetModuleHandleW(name: *const u16) -> HMODULE;
    pub fn GetProcAddress(m: HMODULE, name: *const u8) -> *mut c_void;
    pub fn GetSystemDirectoryW(buf: *mut u16, len: u32) -> u32;
    pub fn GetModuleFileNameW(m: HMODULE, buf: *mut u16, len: u32) -> u32;
    pub fn DisableThreadLibraryCalls(m: HMODULE) -> i32;
    pub fn VirtualProtect(addr: *mut c_void, size: usize, new: u32, old: *mut u32) -> i32;
    pub fn FlushInstructionCache(proc_: *mut c_void, addr: *const c_void, size: usize) -> i32;
    pub fn GetCurrentProcess() -> *mut c_void;
    pub fn VirtualAlloc(addr: *mut c_void, size: usize, kind: u32, protect: u32) -> *mut c_void;
    pub fn VirtualQuery(addr: *const c_void, info: *mut MEMORY_BASIC_INFORMATION, len: usize) -> usize;
}

#[repr(C)]
pub struct MEMORY_BASIC_INFORMATION {
    pub BaseAddress: *mut c_void,
    pub AllocationBase: *mut c_void,
    pub AllocationProtect: u32,
    pub PartitionId: u16,
    pub RegionSize: usize,
    pub State: u32,
    pub Protect: u32,
    pub Type: u32,
}

pub const MEM_COMMIT: u32 = 0x1000;
pub const MEM_RESERVE: u32 = 0x2000;
pub const MEM_FREE: u32 = 0x10000;

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// Overwrites code/data in a loaded module.
pub unsafe fn write_bytes(dst: *mut u8, bytes: &[u8]) {
    let mut old = 0u32;
    VirtualProtect(dst.cast(), bytes.len(), PAGE_EXECUTE_READWRITE, &mut old);
    core::ptr::copy_nonoverlapping(bytes.as_ptr(), dst, bytes.len());
    VirtualProtect(dst.cast(), bytes.len(), old, &mut old);
    FlushInstructionCache(GetCurrentProcess(), dst.cast(), bytes.len());
}

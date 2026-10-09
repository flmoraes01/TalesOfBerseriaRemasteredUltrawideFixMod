//! Tales of Berseria Remastered — ultrawide fix.
//!
//! The game's original engine lives in `TLSampleViewerTO12.dll`; Unity only
//! replays its draw commands. The engine only knows 16:9 and 16:10 (anything
//! else is snapped to 16:9 and stretched), so this mod patches that DLL in
//! memory as soon as it is mapped:
//!
//! * the display and camera aspect use the real width/height ratio, so 3D is
//!   rendered at the native aspect instead of a stretched 16:9 image;
//! * the engine's `SetAspect` is hooked to size the UI canvas (a 1280x720
//!   space centred on the origin) to the screen, keeping the HUD unstretched
//!   and centred, and to size full-screen quads (fades, menu backdrops) to
//!   cover the whole screen.

mod proxy;
mod win;

use core::ffi::c_void;
use std::fs::File;
use std::io::Write;
use std::sync::Mutex;
use win::*;

const ENGINE_DLL: &str = "TLSampleViewerTO12.dll";
const ASPECT_16_9: f32 = 16.0 / 9.0;

static LOG: Mutex<Option<File>> = Mutex::new(None);

macro_rules! log {
    ($($t:tt)*) => {{
        if let Ok(mut g) = LOG.lock() {
            if let Some(f) = g.as_mut() {
                let _ = writeln!(f, $($t)*);
                let _ = f.flush();
            }
        }
    }};
}

/// Byte pattern; `None` = wildcard. Written as "F3 0F ?? ..".
fn parse_pattern(s: &str) -> Vec<Option<u8>> {
    s.split_whitespace()
        .map(|b| if b == "??" { None } else { Some(u8::from_str_radix(b, 16).unwrap()) })
        .collect()
}

fn find(image: &[u8], pattern: &str) -> Option<usize> {
    let pat = parse_pattern(pattern);
    image
        .windows(pat.len())
        .position(|w| w.iter().zip(&pat).all(|(b, p)| p.map_or(true, |p| p == *b)))
}

/// Address referenced by a RIP-relative operand whose disp32 sits at `disp`
/// and whose instruction ends at `end`.
unsafe fn rip_target(disp: *const u8, end: *const u8) -> *mut u8 {
    end.offset((disp as *const i32).read_unaligned() as isize) as *mut u8
}

/// rel32 from the end of an instruction (`end`) to `target`.
fn rel32(end: *const u8, target: *const u8) -> [u8; 4] {
    ((target as isize - end as isize) as i32).to_le_bytes()
}

/// `movaps xmm2, xmm1` padded with NOPs to `len` bytes: keeps the computed
/// width/height ratio instead of snapping it to 16:9.
fn keep_real_aspect(len: usize) -> Vec<u8> {
    let mut code = vec![0x0F, 0x28, 0xD1];
    code.resize(len, 0x90);
    code
}

/// Everything the `SetAspect` hook needs, filled once the engine is patched.
struct Hook {
    set_aspect: unsafe extern "system" fn(f32),
    /// 16:9 UI canvas size (two f32) in the engine's .data.
    ortho_16_9: *mut [f32; 2],
    /// Vertical scale read by the patched full-screen quad builders.
    quad_y_scale: *mut f32,
    /// `±640` immediates of the full-screen quad vertices (address, sign).
    quad_x: Vec<(*mut u8, f32)>,
}
unsafe impl Send for Hook {}

static HOOK: Mutex<Option<Hook>> = Mutex::new(None);

/// UI layout for a given aspect: (canvas size, quad x half-extent, quad y
/// scale). The canvas grows along the long axis so the 1280x720 HUD keeps its
/// proportions and stays centred.
fn ui_layout(aspect: f32) -> ([f32; 2], f32, f32) {
    if aspect >= ASPECT_16_9 {
        ([720.0 * aspect, 720.0], 360.0 * aspect, 1.0)
    } else {
        ([1280.0, 1280.0 / aspect], 640.0, ASPECT_16_9 / aspect)
    }
}

unsafe extern "system" fn set_aspect_hook(aspect: f32) {
    let guard = HOOK.lock().unwrap();
    let hook = guard.as_ref().unwrap();
    let (canvas, quad_x, quad_y) = ui_layout(aspect);
    write_bytes(hook.ortho_16_9.cast(), &core::mem::transmute::<[f32; 2], [u8; 8]>(canvas));
    *hook.quad_y_scale = quad_y;
    for &(imm, sign) in &hook.quad_x {
        write_bytes(imm, &(sign * quad_x).to_le_bytes());
    }
    let set_aspect = hook.set_aspect;
    drop(guard);
    log!("SetAspect({aspect:.4}) -> UI canvas {:.0}x{:.0}", canvas[0], canvas[1]);
    set_aspect(aspect);
}

/// Commits one page within rel32 reach of `near` (for a jump stub and data).
unsafe fn alloc_near(near: *mut u8) -> Option<*mut u8> {
    const GRAN: usize = 0x10000;
    const REACH: usize = 0x7000_0000;
    let hi = near as usize + REACH;
    let mut addr = (near as usize).saturating_sub(REACH) & !(GRAN - 1);
    while addr < hi {
        let mut mbi: MEMORY_BASIC_INFORMATION = core::mem::zeroed();
        if VirtualQuery(addr as *const c_void, &mut mbi, core::mem::size_of_val(&mbi)) == 0 {
            return None;
        }
        let region_end = mbi.BaseAddress as usize + mbi.RegionSize;
        let candidate = (mbi.BaseAddress as usize + GRAN - 1) & !(GRAN - 1);
        if mbi.State == MEM_FREE && candidate + 0x1000 <= region_end {
            let p = VirtualAlloc(candidate as *mut c_void, 0x1000, MEM_COMMIT | MEM_RESERVE, PAGE_EXECUTE_READWRITE);
            if !p.is_null() {
                return Some(p.cast());
            }
        }
        addr = region_end;
    }
    None
}

// Display aspect (resolution handler):
//   aspect = w / h;  if |aspect - 1.6| >= 0.01 { aspect = 16/9 }
const SIG_DISPLAY_ASPECT: &str = "F3 0F 5E C8 F3 0F 10 05 ?? ?? ?? ?? F3 0F 5C CA 0F 54 0D ?? ?? ?? ?? 0F 2F C1 77 08 F3 0F 10 15 ?? ?? ?? ?? F3 0F 11 15";
// Camera aspect: same snapping, then `movaps xmm0, xmm2; call SetAspect`.
const SIG_CAMERA_ASPECT: &str = "F3 0F 5E C8 F3 0F 10 15 ?? ?? ?? ?? F3 0F 5C CA 0F 54 0D ?? ?? ?? ?? F3 0F 10 05 ?? ?? ?? ?? 0F 2F C1 77 08 F3 0F 10 15 ?? ?? ?? ?? F3 0F 11 53 40 0F 28 C2 E8";
// Inside SetAspect: UI canvas is 1280x800 for 16:10, else this 1280x720.
const SIG_UI_CANVAS: &str = "F2 0F 10 05 ?? ?? ?? ?? C6 40 24 01 F2 0F 11 80 F0 00 00 00";
// Full-screen quads: y scale = (16/9) / camera aspect, x = ±640.
//   `movss xmm2, [16/9]; divss xmm2, xmm0` then vertices built right after.
const SIG_QUAD_A: &str = "F3 0F 10 40 14 0F 2F C6 76 0C F3 0F 10 15 ?? ?? ?? ?? F3 0F 5E D0 48 8B 45 ?? 0F 28 C2 F3 0F 59 05 ?? ?? ?? ?? C7 00 00 00 20 C4";
//   Same with `movss xmm1, [16/9]; divss xmm1, xmm2; jmp back`.
const SIG_QUAD_B_SCALE: &str = "F3 0F 10 50 14 0F 57 C0 0F 2F D0 0F 86 ?? ?? ?? ?? F3 0F 10 0D ?? ?? ?? ?? F3 0F 5E CA E9";
const SIG_QUAD_B_VERTS: &str = "F3 0F 10 0D ?? ?? ?? ?? 0F 28 C1 F3 0F 59 05 ?? ?? ?? ?? C7 02 00 00 20 C4";

/// Locates the four `±640.0f` immediates of a quad builder starting at `from`.
fn quad_x_immediates(image: &[u8], from: usize, len: usize) -> Option<Vec<(usize, f32)>> {
    let (neg, pos) = ((-640.0f32).to_le_bytes(), 640.0f32.to_le_bytes());
    let window = image.get(from..from + len)?;
    let found: Vec<(usize, f32)> = (0..window.len().saturating_sub(4))
        .filter_map(|i| match &window[i..i + 4] {
            w if w == neg => Some((from + i, -1.0)),
            w if w == pos => Some((from + i, 1.0)),
            _ => None,
        })
        .collect();
    (found.len() == 4).then_some(found)
}

unsafe fn patch_engine(base: *mut u8, size: usize) {
    log!("{ENGINE_DLL} mapped at {base:p} (size {size:#x})");
    let image = core::slice::from_raw_parts(base as *const u8, size);
    let sig = |name: &str, pattern: &str| {
        let at = find(image, pattern);
        if at.is_none() {
            log!("[!!] {name}: signature not found (game updated?)");
        }
        at
    };

    // 1. Display aspect.
    if let Some(at) = sig("display aspect", SIG_DISPLAY_ASPECT) {
        write_bytes(base.add(at + 4), &keep_real_aspect(32));
        log!("[ok] display aspect @ +{:#x}", at + 4);
    }

    // 2. Camera aspect + SetAspect hook + full-screen quads (all or nothing,
    //    so the HUD never ends up half-patched).
    let (Some(cam), Some(canvas), Some(qa), Some(qb_scale), Some(qb_verts)) = (
        sig("camera aspect", SIG_CAMERA_ASPECT),
        sig("UI canvas", SIG_UI_CANVAS),
        sig("full-screen quad A", SIG_QUAD_A),
        sig("full-screen quad B scale", SIG_QUAD_B_SCALE),
        sig("full-screen quad B vertices", SIG_QUAD_B_VERTS),
    ) else {
        return;
    };
    let (Some(xa), Some(xb)) = (quad_x_immediates(image, qa + 37, 0x50), quad_x_immediates(image, qb_verts + 19, 0x80)) else {
        log!("[!!] full-screen quad vertices: unexpected layout");
        return;
    };
    let Some(page) = alloc_near(base) else {
        log!("[!!] could not allocate memory near the engine");
        return;
    };

    // Page layout: +0 absolute jump to the hook, +0x10 quad y scale.
    let mut stub = vec![0xFF, 0x25, 0, 0, 0, 0];
    stub.extend_from_slice(&(set_aspect_hook as unsafe extern "system" fn(f32) as usize).to_le_bytes());
    write_bytes(page, &stub);
    let quad_y_scale = page.add(0x10).cast::<f32>();
    *quad_y_scale = 1.0;

    let call = base.add(cam + SIG_CAMERA_ASPECT.split_whitespace().count() - 1);
    *HOOK.lock().unwrap() = Some(Hook {
        set_aspect: core::mem::transmute(rip_target(call.add(1), call.add(5))),
        ortho_16_9: rip_target(base.add(canvas + 4), base.add(canvas + 8)).cast(),
        quad_y_scale,
        quad_x: xa.into_iter().chain(xb).map(|(off, s)| (base.add(off), s)).collect(),
    });

    // Quad builders: `movss xmmN, [16/9]; divss ...` -> `movss xmmN, [quad_y_scale]`.
    for (name, movss) in [("quad A", base.add(qa + 10)), ("quad B", base.add(qb_scale + 17))] {
        let mut code = vec![0xF3, 0x0F, 0x10, *movss.add(3)];
        code.extend_from_slice(&rel32(movss.add(8), quad_y_scale.cast()));
        code.resize(12, 0x90);
        write_bytes(movss, &code);
        log!("[ok] {name} scale @ +{:#x}", movss as usize - base as usize);
    }

    write_bytes(call.add(1), &rel32(call.add(5), page));
    write_bytes(base.add(cam + 4), &keep_real_aspect(40));
    log!("[ok] camera aspect @ +{:#x}, SetAspect hooked @ +{:#x}", cam + 4, call as usize - base as usize);
}

unsafe fn base_name(s: *const UNICODE_STRING) -> String {
    let s = &*s;
    String::from_utf16_lossy(core::slice::from_raw_parts(s.Buffer, (s.Length / 2) as usize))
}

unsafe extern "system" fn on_dll(reason: u32, data: *const LDR_DLL_NOTIFICATION_DATA, _: *mut c_void) {
    if reason != LDR_DLL_NOTIFICATION_REASON_LOADED {
        return;
    }
    let d = &*data;
    if base_name(d.BaseDllName).eq_ignore_ascii_case(ENGINE_DLL) {
        patch_engine(d.DllBase.cast(), d.SizeOfImage as usize);
    }
}

unsafe fn module_dir(module: HMODULE) -> std::path::PathBuf {
    let mut buf = [0u16; 1024];
    let n = GetModuleFileNameW(module, buf.as_mut_ptr(), buf.len() as u32) as usize;
    let mut p = std::path::PathBuf::from(String::from_utf16_lossy(&buf[..n]));
    p.pop();
    p
}

unsafe fn init(module: HMODULE) {
    // Only the game process (not the Steam launcher) loads the engine.
    let mut exe = [0u16; 1024];
    let n = GetModuleFileNameW(core::ptr::null_mut(), exe.as_mut_ptr(), exe.len() as u32) as usize;
    if !String::from_utf16_lossy(&exe[..n]).to_ascii_lowercase().ends_with("_game.exe") {
        return;
    }

    *LOG.lock().unwrap() = File::create(module_dir(module).join("TOBUltrawide.log")).ok();
    log!("TOB Ultrawide {} loaded", env!("CARGO_PKG_VERSION"));

    let ntdll = GetModuleHandleW(wide("ntdll.dll").as_ptr());
    let reg = GetProcAddress(ntdll, b"LdrRegisterDllNotification\0".as_ptr());
    if reg.is_null() {
        log!("[!!] LdrRegisterDllNotification unavailable");
        return;
    }
    let reg: LdrRegisterDllNotificationFn = core::mem::transmute(reg);
    let mut cookie = core::ptr::null_mut();
    let status = reg(0, on_dll, core::ptr::null_mut(), &mut cookie);
    log!("waiting for {ENGINE_DLL} (status {status:#x})");
}

#[no_mangle]
pub unsafe extern "system" fn DllMain(module: HMODULE, reason: u32, _: *mut c_void) -> i32 {
    if reason == DLL_PROCESS_ATTACH {
        DisableThreadLibraryCalls(module);
        if !proxy::init() {
            return 0;
        }
        init(module);
    }
    1
}

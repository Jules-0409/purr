// SPDX-License-Identifier: GPL-3.0-only
//! Windows-only placement and light-dismiss for the self-drawn tray popover.
//!
//! Placement follows the native notification-area menu: sit just outside the taskbar
//! (bottom / top / left / right, read from `SHAppBarMessage(ABM_GETTASKBARPOS)` and the
//! monitor work area), anchor on the clicked icon along the taskbar, clamp to the work
//! area, and keep clear of the "hidden icons" overflow flyout when it is open.
//!
//! Dismissal: a low-level mouse hook closes the popover on any button press outside it
//! (taskbar, desktop, other windows, the overflow flyout), a poll closes it on Esc and when
//! it loses the foreground, and the tray toggle ignores the click that just closed it.
//! Hover dismissal: once the pointer has been inside the popover, leaving it (and the Purr
//! tray icon / hidden-icons flyout) for about a second closes it; a popover the pointer never
//! enters closes by itself after about three seconds, like an unused tray menu.
//! All coordinates are physical pixels (the process is per-monitor DPI aware).

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::time::{Duration, Instant};

#[repr(C)]
#[derive(Copy, Clone, Default, Debug, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    fn intersects(&self, o: &Rect) -> bool {
        self.left < o.right && o.left < self.right && self.top < o.bottom && o.top < self.bottom
    }
    fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.left && x < self.right && y >= self.top && y < self.bottom
    }
    fn is_empty(&self) -> bool {
        self.right <= self.left || self.bottom <= self.top
    }
}

#[repr(C)]
#[derive(Copy, Clone, Default)]
struct Point {
    x: i32,
    y: i32,
}

#[repr(C)]
#[derive(Copy, Clone, Default)]
struct MonitorInfo {
    cb_size: u32,
    rc_monitor: Rect,
    rc_work: Rect,
    dw_flags: u32,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct AppBarData {
    cb_size: u32,
    h_wnd: isize,
    u_callback_message: u32,
    u_edge: u32,
    rc: Rect,
    l_param: isize,
}

#[repr(C)]
#[derive(Copy, Clone)]
struct MsLlHookStruct {
    pt: Point,
    mouse_data: u32,
    flags: u32,
    time: u32,
    dw_extra_info: usize,
}

type HookProc = unsafe extern "system" fn(i32, usize, isize) -> isize;

#[link(name = "user32")]
unsafe extern "system" {
    fn GetCursorPos(lp_point: *mut Point) -> i32;
    fn MonitorFromPoint(pt: Point, dw_flags: u32) -> *mut c_void;
    fn GetMonitorInfoW(h_monitor: *mut c_void, lpmi: *mut MonitorInfo) -> i32;
    fn FindWindowW(class: *const u16, title: *const u16) -> isize;
    fn IsWindowVisible(hwnd: isize) -> i32;
    fn GetWindowRect(hwnd: isize, rect: *mut Rect) -> i32;
    fn GetForegroundWindow() -> isize;
    fn SetForegroundWindow(hwnd: isize) -> i32;
    fn BringWindowToTop(hwnd: isize) -> i32;
    fn GetWindowThreadProcessId(hwnd: isize, pid: *mut u32) -> u32;
    fn AttachThreadInput(attach: u32, to: u32, f_attach: i32) -> i32;
    fn GetAsyncKeyState(v_key: i32) -> i16;
    fn GetDpiForWindow(hwnd: isize) -> u32;
    fn SetWindowsHookExW(id: i32, proc_: HookProc, hmod: isize, thread_id: u32) -> isize;
    fn UnhookWindowsHookEx(hook: isize) -> i32;
    fn CallNextHookEx(hook: isize, code: i32, w: usize, l: isize) -> isize;
}

#[link(name = "kernel32")]
unsafe extern "system" {
    fn GetCurrentThreadId() -> u32;
    fn GetModuleHandleW(name: *const u16) -> isize;
}

#[link(name = "shell32")]
unsafe extern "system" {
    fn SHAppBarMessage(msg: u32, data: *mut AppBarData) -> usize;
}

#[link(name = "dwmapi")]
unsafe extern "system" {
    fn DwmGetWindowAttribute(hwnd: isize, attr: u32, pv: *mut c_void, cb: u32) -> i32;
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Edge {
    Left,
    Top,
    Right,
    Bottom,
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// The open "hidden icons" overflow flyout, if any (Windows 11 XAML island or the
/// classic Windows 10 `NotifyIconOverflowWindow`).
pub fn overflow_rect() -> Option<Rect> {
    for class in [
        "TopLevelWindowForOverflowXamlIsland",
        "NotifyIconOverflowWindow",
    ] {
        let class = wide(class);
        unsafe {
            let hwnd = FindWindowW(class.as_ptr(), std::ptr::null());
            if hwnd == 0 || IsWindowVisible(hwnd) == 0 {
                continue;
            }
            let mut cloaked: u32 = 0;
            let _ = DwmGetWindowAttribute(
                hwnd,
                14, // DWMWA_CLOAKED
                (&mut cloaked as *mut u32).cast::<c_void>(),
                std::mem::size_of::<u32>() as u32,
            );
            if cloaked != 0 {
                continue;
            }
            let mut r = Rect::default();
            if GetWindowRect(hwnd, &mut r) != 0 && !r.is_empty() {
                return Some(r);
            }
        }
    }
    None
}

fn monitor_for(pt: (i32, i32)) -> (Rect, Rect) {
    unsafe {
        let hmon = MonitorFromPoint(Point { x: pt.0, y: pt.1 }, 2 /* NEAREST */);
        let mut mi = MonitorInfo {
            cb_size: std::mem::size_of::<MonitorInfo>() as u32,
            ..Default::default()
        };
        if !hmon.is_null() && GetMonitorInfoW(hmon, &mut mi) != 0 {
            (mi.rc_monitor, mi.rc_work)
        } else {
            let r = Rect {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1032,
            };
            (r, r)
        }
    }
}

/// Taskbar edge and rectangle on the monitor `mon`. Falls back to comparing the work area
/// with the monitor when the shell reports the taskbar on another monitor (or not at all).
fn taskbar(mon: &Rect, work: &Rect) -> (Edge, Option<Rect>) {
    let mut abd = AppBarData {
        cb_size: std::mem::size_of::<AppBarData>() as u32,
        h_wnd: 0,
        u_callback_message: 0,
        u_edge: 3,
        rc: Rect::default(),
        l_param: 0,
    };
    let ok = unsafe {
        SHAppBarMessage(5 /* ABM_GETTASKBARPOS */, &mut abd)
    } != 0;
    if ok && abd.rc.intersects(mon) {
        let edge = match abd.u_edge {
            0 => Edge::Left,
            1 => Edge::Top,
            2 => Edge::Right,
            _ => Edge::Bottom,
        };
        return (edge, Some(abd.rc));
    }
    let edge = if work.bottom < mon.bottom {
        Edge::Bottom
    } else if work.top > mon.top {
        Edge::Top
    } else if work.left > mon.left {
        Edge::Left
    } else if work.right < mon.right {
        Edge::Right
    } else {
        Edge::Bottom
    };
    (edge, None)
}

pub fn cursor() -> (i32, i32) {
    let mut p = Point::default();
    unsafe {
        let _ = GetCursorPos(&mut p);
    }
    (p.x, p.y)
}

/// Pure placement: returns (left, top, upward).
#[allow(clippy::too_many_arguments)]
pub fn place_in(
    pt: (i32, i32),
    width: i32,
    height: i32,
    margin: i32,
    mon: Rect,
    work: Rect,
    edge: Edge,
    bar: Option<Rect>,
    avoid: Option<Rect>,
) -> (i32, i32, bool) {
    // Usable area: the work area, further trimmed by an auto-hide taskbar that overlaps it.
    let mut area = work;
    if let Some(b) = bar {
        if b.intersects(&work) {
            match edge {
                Edge::Bottom => area.bottom = area.bottom.min(b.top),
                Edge::Top => area.top = area.top.max(b.bottom),
                Edge::Left => area.left = area.left.max(b.right),
                Edge::Right => area.right = area.right.min(b.left),
            }
        }
    }
    let min_x = area.left + margin;
    let max_x = (area.right - width - margin).max(min_x);
    let min_y = area.top + margin;
    let max_y = (area.bottom - height - margin).max(min_y);
    let (mut left, mut top) = match edge {
        Edge::Bottom => ((pt.0 - width / 2).clamp(min_x, max_x), max_y),
        Edge::Top => ((pt.0 - width / 2).clamp(min_x, max_x), min_y),
        Edge::Left => (min_x, (pt.1 - height / 2).clamp(min_y, max_y)),
        Edge::Right => (max_x, (pt.1 - height / 2).clamp(min_y, max_y)),
    };
    if let Some(ov) = avoid {
        let r = Rect {
            left,
            top,
            right: left + width,
            bottom: top + height,
        };
        if r.intersects(&ov) {
            let before = ov.left - margin - width; // to the left of the flyout
            let after = ov.right + margin; // to the right of the flyout
            let above = ov.top - margin - height;
            let below = ov.bottom + margin;
            match edge {
                Edge::Bottom | Edge::Top => {
                    if before >= min_x {
                        left = before;
                    } else if after <= max_x {
                        left = after;
                    } else if edge == Edge::Bottom {
                        top = above.max(min_y);
                    } else {
                        top = below.min(max_y);
                    }
                }
                Edge::Left | Edge::Right => {
                    if edge == Edge::Left && after <= max_x {
                        left = after;
                    } else if edge == Edge::Right && before >= min_x {
                        left = before;
                    } else if above >= min_y {
                        top = above;
                    } else {
                        top = below.min(max_y);
                    }
                }
            }
        }
    }
    let mid_y = (mon.top + mon.bottom) / 2;
    let upward = match edge {
        Edge::Bottom => true,
        Edge::Top => false,
        Edge::Left | Edge::Right => pt.1 >= mid_y,
    };
    (left, top, upward)
}

thread_local! {
    static AVOID: std::cell::Cell<Option<Rect>> = const { std::cell::Cell::new(None) };
}

/// Remember the overflow flyout as it is when the popover opens, so later re-placements
/// (height changes) do not jump if the flyout closes meanwhile.
pub fn snapshot_overflow() {
    AVOID.with(|c| c.set(overflow_rect()));
}

pub fn place(pt: (i32, i32), width: i32, height: i32, margin: i32) -> (i32, i32, bool) {
    let pt = if pt == (0, 0) { cursor() } else { pt };
    let (mon, work) = monitor_for(pt);
    let (edge, bar) = taskbar(&mon, &work);
    let avoid = AVOID.with(|c| c.get());
    place_in(pt, width, height, margin, mon, work, edge, bar, avoid)
}

/// Bring the popover to the foreground so it receives focus / deactivation (the classic
/// TrackPopupMenu requirement). Uses AttachThreadInput when the plain call is refused.
pub fn force_foreground(hwnd: isize) {
    unsafe {
        if SetForegroundWindow(hwnd) != 0 && GetForegroundWindow() == hwnd {
            return;
        }
        let fg = GetForegroundWindow();
        let me = GetCurrentThreadId();
        let other = if fg != 0 {
            GetWindowThreadProcessId(fg, std::ptr::null_mut())
        } else {
            0
        };
        let attached = other != 0 && other != me && AttachThreadInput(other, me, 1) != 0;
        let _ = BringWindowToTop(hwnd);
        let _ = SetForegroundWindow(hwnd);
        if attached {
            let _ = AttachThreadInput(other, me, 0);
        }
    }
}

// ---- light dismiss -------------------------------------------------------------------

static POPUP: AtomicIsize = AtomicIsize::new(0);
static HOOK: AtomicIsize = AtomicIsize::new(0);
static OUTSIDE: AtomicBool = AtomicBool::new(false);

thread_local! {
    static LAST_AUTO: std::cell::Cell<Option<Instant>> = const { std::cell::Cell::new(None) };
    static WATCH: std::cell::Cell<Watch> = const { std::cell::Cell::new(Watch::IDLE) };
    static ANCHOR: std::cell::Cell<(i32, i32)> = const { std::cell::Cell::new((0, 0)) };
}

/// Pointer outside the popover (and off the tray icon / flyout) this long closes it.
const HOVER_LEAVE: Duration = Duration::from_millis(1000);
/// A popover the pointer never enters closes after this long.
const HOVER_NEVER: Duration = Duration::from_millis(3000);

#[derive(Copy, Clone)]
struct Watch {
    had_fg: bool,
    esc: bool,
    opened: Option<Instant>,
    entered: bool,
    out_since: Option<Instant>,
}

impl Watch {
    const IDLE: Watch = Watch {
        had_fg: false,
        esc: true,
        opened: None,
        entered: false,
        out_since: None,
    };
}

/// Where the tray icon was clicked (physical px); the pointer resting there does not count
/// as leaving the popover.
pub fn set_anchor(pt: (i32, i32)) {
    let pt = if pt == (0, 0) { cursor() } else { pt };
    ANCHOR.with(|c| c.set(pt));
}

/// Pure hover rule: returns (close, entered, out_since).
pub fn hover_step(
    now: Instant,
    opened: Instant,
    entered: bool,
    out_since: Option<Instant>,
    in_popup: bool,
    in_safe: bool,
) -> (bool, bool, Option<Instant>) {
    if in_popup {
        return (false, true, None);
    }
    if !entered {
        return (now.duration_since(opened) >= HOVER_NEVER, false, None);
    }
    if in_safe {
        return (false, true, None);
    }
    let since = out_since.unwrap_or(now);
    (now.duration_since(since) >= HOVER_LEAVE, true, Some(since))
}

unsafe extern "system" fn mouse_hook(code: i32, w: usize, l: isize) -> isize {
    if code >= 0 {
        // WM_LBUTTONDOWN, WM_RBUTTONDOWN, WM_MBUTTONDOWN, WM_XBUTTONDOWN
        if matches!(w, 0x0201 | 0x0204 | 0x0207 | 0x020B) {
            let hwnd = POPUP.load(Ordering::Relaxed);
            if hwnd != 0 {
                let info = unsafe { &*(l as *const MsLlHookStruct) };
                let mut r = Rect::default();
                let inside =
                    unsafe { GetWindowRect(hwnd, &mut r) } != 0 && r.contains(info.pt.x, info.pt.y);
                if !inside && !OUTSIDE.swap(true, Ordering::Relaxed) {
                    let _ = slint::invoke_from_event_loop(|| {
                        mark_auto_dismiss();
                        super::dismiss();
                    });
                }
            }
        }
    }
    unsafe { CallNextHookEx(0, code, w, l) }
}

/// Start watching the popover window: outside clicks, Esc, lost foreground.
pub fn arm(hwnd: isize) {
    POPUP.store(hwnd, Ordering::Relaxed);
    OUTSIDE.store(false, Ordering::Relaxed);
    // Esc may already be down if the user opened the menu with a keyboard; only a new press counts.
    let esc_down = unsafe { GetAsyncKeyState(0x1B) } as u16 & 0x8000 != 0;
    WATCH.with(|w| {
        w.set(Watch {
            esc: esc_down,
            opened: Some(Instant::now()),
            ..Watch::IDLE
        })
    });
    if HOOK.load(Ordering::Relaxed) == 0 {
        let hook = unsafe {
            SetWindowsHookExW(
                14, /* WH_MOUSE_LL */
                mouse_hook,
                GetModuleHandleW(std::ptr::null()),
                0,
            )
        };
        HOOK.store(hook, Ordering::Relaxed);
    }
}

pub fn armed() -> bool {
    POPUP.load(Ordering::Relaxed) != 0
}

/// Stop watching (popover closed).
pub fn disarm() {
    POPUP.store(0, Ordering::Relaxed);
    let hook = HOOK.swap(0, Ordering::Relaxed);
    if hook != 0 {
        unsafe {
            let _ = UnhookWindowsHookEx(hook);
        }
    }
}

fn mark_auto_dismiss() {
    LAST_AUTO.with(|c| c.set(Some(Instant::now())));
}

/// True right after the popover was closed by an outside press: the same press on the
/// tray icon must not reopen it (toggle semantics).
pub fn just_auto_dismissed() -> bool {
    LAST_AUTO.with(|c| {
        c.get()
            .map(|t| t.elapsed() < Duration::from_millis(400))
            .unwrap_or(false)
    })
}

/// Called from the popover's frame timer. Returns true when it should close.
pub fn poll() -> bool {
    let hwnd = POPUP.load(Ordering::Relaxed);
    if hwnd == 0 {
        return false;
    }
    let mut w = WATCH.with(|c| c.get());
    let mut close = false;
    let esc = unsafe { GetAsyncKeyState(0x1B) } as u16 & 0x8000 != 0;
    if esc && !w.esc {
        close = true;
    }
    w.esc = esc;
    let fg = unsafe { GetForegroundWindow() };
    if fg == hwnd {
        w.had_fg = true;
    } else if w.had_fg && fg != 0 {
        // Lost activation to another window (Alt+Tab, Win key, a click the hook missed).
        close = true;
        mark_auto_dismiss();
    }
    // Hover dismissal (does not mark an auto-dismiss: a click on the icon right after it
    // should open the popover again).
    if !close {
        let (cx, cy) = cursor();
        let mut r = Rect::default();
        let in_popup = unsafe { GetWindowRect(hwnd, &mut r) } != 0 && r.contains(cx, cy);
        let in_safe = !in_popup && {
            let dpi = unsafe { GetDpiForWindow(hwnd) };
            let half = 20 * if dpi == 0 { 96 } else { dpi as i32 } / 96;
            let (ax, ay) = ANCHOR.with(|c| c.get());
            let icon = Rect {
                left: ax - half,
                top: ay - half,
                right: ax + half,
                bottom: ay + half,
            };
            icon.contains(cx, cy) || overflow_rect().is_some_and(|o| o.contains(cx, cy))
        };
        let now = Instant::now();
        let opened = *w.opened.get_or_insert(now);
        let (c, entered, out_since) =
            hover_step(now, opened, w.entered, w.out_since, in_popup, in_safe);
        w.entered = entered;
        w.out_since = out_since;
        close = c;
    }
    WATCH.with(|c| c.set(w));
    close
}

#[cfg(test)]
mod tests {
    use super::*;

    // 1920x1200 physical monitor at 125 %, 48 px bottom taskbar.
    const MON: Rect = Rect {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1200,
    };
    const WORK: Rect = Rect {
        left: 0,
        top: 0,
        right: 1920,
        bottom: 1152,
    };
    const BAR: Rect = Rect {
        left: 0,
        top: 1152,
        right: 1920,
        bottom: 1200,
    };

    #[test]
    fn bottom_taskbar_sits_above_bar_anchored_on_icon() {
        let (l, t, up) = place_in(
            (1700, 1160),
            385,
            600,
            10,
            MON,
            WORK,
            Edge::Bottom,
            Some(BAR),
            None,
        );
        assert!(up);
        assert_eq!(t + 600, 1152 - 10);
        assert_eq!(l, 1700 - 192);
    }

    #[test]
    fn clamps_to_right_edge() {
        let (l, _, _) = place_in(
            (1900, 1160),
            385,
            600,
            10,
            MON,
            WORK,
            Edge::Bottom,
            Some(BAR),
            None,
        );
        assert_eq!(l + 385, 1920 - 10);
    }

    #[test]
    fn avoids_open_overflow_flyout() {
        let ov = Rect {
            left: 1600,
            top: 1000,
            right: 1800,
            bottom: 1140,
        };
        let (l, t, _) = place_in(
            (1700, 1080),
            385,
            600,
            10,
            MON,
            WORK,
            Edge::Bottom,
            Some(BAR),
            Some(ov),
        );
        let r = Rect {
            left: l,
            top: t,
            right: l + 385,
            bottom: t + 600,
        };
        assert!(!r.intersects(&ov));
        assert!(t + 600 <= 1152);
    }

    #[test]
    fn hover_rules() {
        let t0 = Instant::now();
        let ms = |n| t0 + Duration::from_millis(n);
        // never entered: stays until 3 s, then closes even if resting on the icon
        assert!(!hover_step(ms(2900), t0, false, None, false, true).0);
        assert!(hover_step(ms(3000), t0, false, None, false, true).0);
        // entered then left: closes after 1 s outside, cancelled by coming back
        let (c, e, o) = hover_step(ms(500), t0, true, None, false, false);
        assert!(!c && e && o == Some(ms(500)));
        assert!(!hover_step(ms(1400), t0, true, o, false, false).0);
        assert!(hover_step(ms(1500), t0, true, o, false, false).0);
        let (c, _, o2) = hover_step(ms(1200), t0, true, o, true, false);
        assert!(!c && o2.is_none());
        // on the icon / flyout counts as not left
        let (c, _, o3) = hover_step(ms(9000), t0, true, o, false, true);
        assert!(!c && o3.is_none());
        // inside never closes, even long after opening
        assert!(!hover_step(ms(60000), t0, false, None, true, false).0);
    }

    #[test]
    fn top_left_right_taskbars() {
        let work = Rect {
            left: 0,
            top: 48,
            right: 1920,
            bottom: 1200,
        };
        let (_, t, up) = place_in((1700, 20), 385, 600, 10, MON, work, Edge::Top, None, None);
        assert!(!up);
        assert_eq!(t, 58);
        let work = Rect {
            left: 60,
            top: 0,
            right: 1920,
            bottom: 1200,
        };
        let (l, _, _) = place_in((30, 1100), 385, 600, 10, MON, work, Edge::Left, None, None);
        assert_eq!(l, 70);
        let work = Rect {
            left: 0,
            top: 0,
            right: 1860,
            bottom: 1200,
        };
        let (l, _, _) = place_in(
            (1890, 1100),
            385,
            600,
            10,
            MON,
            work,
            Edge::Right,
            None,
            None,
        );
        assert_eq!(l + 385, 1850);
    }
}

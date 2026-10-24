//! Canonical Dark Bento UI for SmartBlink (`SmartBlinkSettingsWnd`, `SmartBlinkAboutWnd`,
//! and `SmartBlinkTrayPopupWnd`, M9/M12, ARCHITECTURE.md §3.8, §3.9).
//!
//! Renders a DPI-aware, double-buffered 32-bit ARGB Bento grid with:
//! - Exact vector rasterization of `assets/smartblink-logo.svg` (with `#00E5FF` glow and
//!   `#8B95A5` 50% group strokes).
//! - Asymmetric 12-column Bento mosaic (`7+5`, `7+5`, `6+6`, `8+4` spans).
//! - Direct 1-click segmented cards, mechanical keycap selectors, pill toggle switches,
//!   and interactive process exclusion chips (no blind cycling required).

use crate::autostart;
use crate::config::{self, Config, FixTrigger, LedMode, NotifyMode};
use crate::hooks;
use crate::signal;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::{Mutex, Once, OnceLock};
use windows::core::{s, w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, BitBlt, CreateCompatibleDC, CreateDIBSection, CreateFontW, DeleteDC, DeleteObject,
    DrawTextW, EndPaint, GetDC, GetDeviceCaps, InvalidateRect, ReleaseDC, SelectObject, SetBkMode,
    SetTextColor, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CLEARTYPE_QUALITY, CLIP_DEFAULT_PRECIS,
    DEFAULT_CHARSET, DIB_RGB_COLORS, DT_CENTER, DT_END_ELLIPSIS, DT_LEFT, DT_NOPREFIX, DT_RIGHT,
    DT_SINGLELINE, DT_VCENTER, DT_WORDBREAK, HBITMAP, HDC, HFONT, HGDIOBJ, LOGPIXELSX,
    OUT_DEFAULT_PRECIS, PAINTSTRUCT, SRCCOPY, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress, LoadLibraryW};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    TrackMouseEvent, TME_LEAVE, TRACKMOUSEEVENT, VK_ESCAPE,
};
use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect,
    GetCursorPos, GetSystemMetrics, IsWindow, KillTimer, LoadCursorW, LoadImageW, PostMessageW,
    RegisterClassW, SetCursor, SetForegroundWindow, SetTimer, ShowWindow, SystemParametersInfoW,
    HICON, HMENU, IDC_ARROW, IDC_HAND, IMAGE_ICON, LR_DEFAULTCOLOR, SM_CXICON, SM_CXSCREEN,
    SM_CXSMICON, SM_CYSCREEN, SPI_GETWORKAREA, SW_HIDE, SW_SHOW, SW_SHOWNORMAL,
    SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, WA_INACTIVE, WINDOW_EX_STYLE, WM_ACTIVATE, WM_CLOSE, WM_CREATE, WM_DESTROY, WM_ERASEBKGND, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP,
    WM_MOUSEMOVE, WM_PAINT, WM_SETCURSOR, WM_TIMER, WNDCLASSW, WS_CAPTION, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_POPUP, WS_SYSMENU, WS_VISIBLE,
};

const WM_MOUSELEAVE_MSG: u32 = 0x02A3;
const IDI_MAIN_RES: usize = 101;
const TIMER_TEST_PULSE: usize = 701;

static SETTINGS_HWND: AtomicIsize = AtomicIsize::new(0);
static ABOUT_HWND: AtomicIsize = AtomicIsize::new(0);
static FLYOUT_HWND: AtomicIsize = AtomicIsize::new(0);
static TRAY_OWNER_HWND: AtomicIsize = AtomicIsize::new(0);
static TEST_PULSE_ACTIVE: AtomicBool = AtomicBool::new(false);

static SETTINGS_CLASS_ONCE: Once = Once::new();
static ABOUT_CLASS_ONCE: Once = Once::new();
static FLYOUT_CLASS_ONCE: Once = Once::new();

static SETTINGS_HOVER: AtomicIsize = AtomicIsize::new(-1);
static ABOUT_HOVER: AtomicIsize = AtomicIsize::new(-1);
static FLYOUT_HOVER: AtomicIsize = AtomicIsize::new(-1);

// Brand & Bento Palette (derived from smartblink-logo.svg: #00E5FF & #8B95A5)
const COL_CANVAS: (u8, u8, u8) = (11, 14, 20); // #0B0E14
const COL_TILE: (u8, u8, u8) = (18, 23, 34); // #121722
const COL_TILE_ALT: (u8, u8, u8) = (15, 20, 30); // #0F141E
const COL_TILE_HERO: (u8, u8, u8) = (13, 22, 35); // #0D1623
const COL_SUB: (u8, u8, u8) = (12, 16, 24); // #0C1018
const COL_SUB_HOVER: (u8, u8, u8) = (23, 31, 46); // #171F2E
const COL_SUB_ACTIVE: (u8, u8, u8) = (12, 41, 56); // #0C2938
const COL_BORDER: (u8, u8, u8) = (30, 38, 54); // #1E2636
const COL_BORDER_HERO: (u8, u8, u8) = (27, 64, 84); // #1B4054
const COL_CYAN: (u8, u8, u8) = (0, 229, 255); // #00E5FF
const COL_CYAN_HOVER: (u8, u8, u8) = (64, 238, 255); // #40EEFF
const COL_SLATE: (u8, u8, u8) = (139, 149, 165); // #8B95A5
const COL_TEXT: (u8, u8, u8) = (241, 245, 249); // #F1F5F9
const COL_TEXT_SUB: (u8, u8, u8) = (148, 163, 184); // #94A3B8
const COL_TEXT_MUTED: (u8, u8, u8) = (100, 116, 139); // #64748B
const COL_AMBER: (u8, u8, u8) = (245, 158, 11); // #F59E0B
const COL_RED_BG: (u8, u8, u8) = (48, 20, 26);
const COL_RED_BORDER: (u8, u8, u8) = (180, 52, 64);

#[inline]
const fn rgb_ref(c: (u8, u8, u8)) -> COLORREF {
    COLORREF((c.0 as u32) | ((c.1 as u32) << 8) | ((c.2 as u32) << 16))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum HitAction {
    TogglePause,
    TriggerTestAlert,
    SetNotify(NotifyMode),
    SetLed(LedMode),
    SetFix(FixTrigger),
    ToggleAutostart,
    ToggleElevatedTask,
    ToggleLastProcess,
    RemoveBlacklistIdx(usize),
    ResetBlacklistDefaults,
    OpenConfigFile,
    OpenAboutWindow,
    OpenSettingsWindow,
    CloseWindow,
    OpenWebsite,
    ExitApp,
}

#[derive(Clone, Copy)]
pub struct HitZone {
    pub id: isize,
    pub rect: RECT,
    pub action: HitAction,
}

pub struct Canvas {
    pub w: i32,
    pub h: i32,
    pub px: Vec<u32>, // 0xAARRGGBB (little-endian BGRA in memory)
}

impl Canvas {
    pub fn new(w: i32, h: i32, bg: (u8, u8, u8)) -> Self {
        let count = (w.max(1) * h.max(1)) as usize;
        let fill = 0xFF00_0000 | ((bg.0 as u32) << 16) | ((bg.1 as u32) << 8) | (bg.2 as u32);
        Self {
            w: w.max(1),
            h: h.max(1),
            px: vec![fill; count],
        }
    }

    #[inline]
    pub fn blend_pixel(&mut self, x: i32, y: i32, c: (u8, u8, u8), alpha: f32) {
        if x < 0 || y < 0 || x >= self.w || y >= self.h || alpha <= 0.002 {
            return;
        }
        let a = alpha.clamp(0.0, 1.0);
        let idx = (y * self.w + x) as usize;
        let dst = self.px[idx];
        let dr = ((dst >> 16) & 0xFF) as f32;
        let dg = ((dst >> 8) & 0xFF) as f32;
        let db = (dst & 0xFF) as f32;
        let inv = 1.0 - a;
        let or = (c.0 as f32 * a + dr * inv + 0.5) as u32;
        let og = (c.1 as f32 * a + dg * inv + 0.5) as u32;
        let ob = (c.2 as f32 * a + db * inv + 0.5) as u32;
        self.px[idx] = 0xFF00_0000 | (or.min(255) << 16) | (og.min(255) << 8) | ob.min(255);
    }

    pub fn fill_round_rect(&mut self, r: RECT, radius: f32, fill: (u8, u8, u8), alpha: f32) {
        let x0 = r.left.max(0);
        let y0 = r.top.max(0);
        let x1 = r.right.min(self.w);
        let y1 = r.bottom.min(self.h);
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let cx = (r.left + r.right) as f32 * 0.5;
        let cy = (r.top + r.bottom) as f32 * 0.5;
        let hw = (r.right - r.left) as f32 * 0.5;
        let hh = (r.bottom - r.top) as f32 * 0.5;
        let rad = radius.min(hw).min(hh).max(0.0);

        for y in y0..y1 {
            let py = y as f32 + 0.5;
            let qy = (py - cy).abs() - (hh - rad);
            for x in x0..x1 {
                let px = x as f32 + 0.5;
                let qx = (px - cx).abs() - (hw - rad);
                let d = if qx <= 0.0 && qy <= 0.0 {
                    qx.max(qy) - rad
                } else {
                    let ox = qx.max(0.0);
                    let oy = qy.max(0.0);
                    (ox * ox + oy * oy).sqrt() - rad
                };
                if d < 0.65 {
                    let cov = if d <= -0.65 {
                        1.0
                    } else {
                        let t = (d + 0.65) / 1.3;
                        1.0 - t * t * (3.0 - 2.0 * t)
                    };
                    self.blend_pixel(x, y, fill, cov * alpha);
                }
            }
        }
    }

    pub fn stroke_round_rect(
        &mut self,
        r: RECT,
        radius: f32,
        thickness: f32,
        stroke: (u8, u8, u8),
        alpha: f32,
    ) {
        let x0 = (r.left - 2).max(0);
        let y0 = (r.top - 2).max(0);
        let x1 = (r.right + 2).min(self.w);
        let y1 = (r.bottom + 2).min(self.h);
        if x1 <= x0 || y1 <= y0 {
            return;
        }
        let cx = (r.left + r.right) as f32 * 0.5;
        let cy = (r.top + r.bottom) as f32 * 0.5;
        let hw = (r.right - r.left) as f32 * 0.5;
        let hh = (r.bottom - r.top) as f32 * 0.5;
        let rad = radius.min(hw).min(hh).max(0.0);
        let half_t = thickness * 0.5;

        for y in y0..y1 {
            let py = y as f32 + 0.5;
            let qy = (py - cy).abs() - (hh - rad);
            for x in x0..x1 {
                let px = x as f32 + 0.5;
                let qx = (px - cx).abs() - (hw - rad);
                let d = if qx <= 0.0 && qy <= 0.0 {
                    qx.max(qy) - rad
                } else {
                    let ox = qx.max(0.0);
                    let oy = qy.max(0.0);
                    (ox * ox + oy * oy).sqrt() - rad
                };
                // Inset border: centered at d = -half_t
                let ring_d = (d + half_t).abs() - half_t;
                if ring_d < 0.65 {
                    let cov = if ring_d <= -0.65 {
                        1.0
                    } else {
                        let t = (ring_d + 0.65) / 1.3;
                        1.0 - t * t * (3.0 - 2.0 * t)
                    };
                    self.blend_pixel(x, y, stroke, cov * alpha);
                }
            }
        }
    }

    pub fn fill_radial_glow_clipped(
        &mut self,
        clip: RECT,
        clip_rad: f32,
        gx: f32,
        gy: f32,
        glow_radius: f32,
        color: (u8, u8, u8),
        max_alpha: f32,
    ) {
        let x0 = clip.left.max(0);
        let y0 = clip.top.max(0);
        let x1 = clip.right.min(self.w);
        let y1 = clip.bottom.min(self.h);
        if x1 <= x0 || y1 <= y0 || glow_radius <= 1.0 {
            return;
        }
        let cx = (clip.left + clip.right) as f32 * 0.5;
        let cy = (clip.top + clip.bottom) as f32 * 0.5;
        let hw = (clip.right - clip.left) as f32 * 0.5;
        let hh = (clip.bottom - clip.top) as f32 * 0.5;
        let rad = clip_rad.min(hw).min(hh).max(0.0);
        let inv_r2 = 1.0 / (glow_radius * glow_radius);

        for y in y0..y1 {
            let py = y as f32 + 0.5;
            let qy = (py - cy).abs() - (hh - rad);
            let dy = py - gy;
            for x in x0..x1 {
                let px = x as f32 + 0.5;
                let dx = px - gx;
                let u2 = (dx * dx + dy * dy) * inv_r2;
                if u2 >= 1.0 {
                    continue;
                }
                let qx = (px - cx).abs() - (hw - rad);
                let d = if qx <= 0.0 && qy <= 0.0 {
                    qx.max(qy) - rad
                } else {
                    let ox = qx.max(0.0);
                    let oy = qy.max(0.0);
                    (ox * ox + oy * oy).sqrt() - rad
                };
                if d < 0.0 {
                    let falloff = (1.0 - u2) * (1.0 - u2);
                    self.blend_pixel(x, y, color, max_alpha * falloff);
                }
            }
        }
    }

    pub fn fill_circle(&mut self, cx: f32, cy: f32, radius: f32, color: (u8, u8, u8), alpha: f32) {
        let x0 = ((cx - radius - 2.0).floor() as i32).max(0);
        let y0 = ((cy - radius - 2.0).floor() as i32).max(0);
        let x1 = ((cx + radius + 2.0).ceil() as i32).min(self.w);
        let y1 = ((cy + radius + 2.0).ceil() as i32).min(self.h);
        for y in y0..y1 {
            let dy = y as f32 + 0.5 - cy;
            for x in x0..x1 {
                let dx = x as f32 + 0.5 - cx;
                let d = (dx * dx + dy * dy).sqrt() - radius;
                if d < 0.65 {
                    let cov = if d <= -0.65 {
                        1.0
                    } else {
                        let t = (d + 0.65) / 1.3;
                        1.0 - t * t * (3.0 - 2.0 * t)
                    };
                    self.blend_pixel(x, y, color, cov * alpha);
                }
            }
        }
    }

    pub fn draw_toggle_switch(&mut self, right_x: i32, center_y: i32, s: f32, is_on: bool) {
        let sw_w = (44.0 * s).round() as i32;
        let sw_h = (24.0 * s).round() as i32;
        let r = RECT {
            left: right_x - sw_w,
            top: center_y - sw_h / 2,
            right: right_x,
            bottom: center_y - sw_h / 2 + sw_h,
        };
        let pill_rad = sw_h as f32 * 0.5;
        if is_on {
            self.fill_round_rect(r, pill_rad, COL_CYAN, 1.0);
            let thumb_cx = r.right as f32 - pill_rad;
            let thumb_cy = (r.top + r.bottom) as f32 * 0.5;
            self.fill_circle(thumb_cx, thumb_cy, (sw_h as f32 * 0.36).max(4.0), COL_CANVAS, 1.0);
        } else {
            self.fill_round_rect(r, pill_rad, (32, 41, 58), 1.0);
            self.stroke_round_rect(r, pill_rad, 1.0 * s, (54, 67, 92), 1.0);
            let thumb_cx = r.left as f32 + pill_rad;
            let thumb_cy = (r.top + r.bottom) as f32 * 0.5;
            self.fill_circle(thumb_cx, thumb_cy, (sw_h as f32 * 0.34).max(4.0), COL_SLATE, 1.0);
        }
    }

    pub fn blit_logo(&mut self, dest_x: i32, dest_y: i32, size: i32) {
        let size_u = size.max(16) as u32;
        let rgba = get_cached_logo_rgba(size_u);
        let side = size_u as i32;
        for y in 0..side {
            let dy = dest_y + y;
            if dy < 0 || dy >= self.h {
                continue;
            }
            for x in 0..side {
                let dx = dest_x + x;
                if dx < 0 || dx >= self.w {
                    continue;
                }
                let idx = ((y * side + x) * 4) as usize;
                let a = rgba[idx + 3] as f32 / 255.0;
                if a > 0.003 {
                    self.blend_pixel(dx, dy, (rgba[idx], rgba[idx + 1], rgba[idx + 2]), a);
                }
            }
        }
    }
}

#[derive(Clone, Copy)]
struct Seg2D {
    ax: f32,
    ay: f32,
    dx: f32,
    dy: f32,
    len_sq: f32,
}

static LOGO_CACHE: OnceLock<Mutex<Vec<(u32, Vec<u8>)>>> = OnceLock::new();
static LOGO_GEOM: OnceLock<(Vec<Seg2D>, Vec<Seg2D>)> = OnceLock::new();

fn build_logo_geom() -> (Vec<Seg2D>, Vec<Seg2D>) {
    fn cubic(p0: (f32, f32), p1: (f32, f32), p2: (f32, f32), p3: (f32, f32), n: usize) -> Vec<(f32, f32)> {
        let mut out = Vec::with_capacity(n + 1);
        for i in 0..=n {
            let t = i as f32 / n as f32;
            let mt = 1.0 - t;
            let x = mt * mt * mt * p0.0
                + 3.0 * mt * mt * t * p1.0
                + 3.0 * mt * t * t * p2.0
                + t * t * t * p3.0;
            let y = mt * mt * mt * p0.1
                + 3.0 * mt * mt * t * p1.1
                + 3.0 * mt * t * t * p2.1
                + t * t * t * p3.1;
            out.push((x, y));
        }
        out
    }

    fn to_segs(pts: &[(f32, f32)]) -> Vec<Seg2D> {
        let mut out = Vec::with_capacity(pts.len().saturating_sub(1));
        for w in pts.windows(2) {
            let (ax, ay) = w[0];
            let (bx, by) = w[1];
            let dx = bx - ax;
            let dy = by - ay;
            out.push(Seg2D {
                ax,
                ay,
                dx,
                dy,
                len_sq: dx * dx + dy * dy,
            });
        }
        out
    }

    let stem = [(100.0f32, 50.0f32), (100.0, 192.0), (164.0, 192.0)];
    let mut conn = vec![(92.0f32, 102.0f32)];
    conn.extend(cubic((100.0, 94.0), (118.0, 78.0), (148.8, 59.6), (144.0, 52.0), 28));

    let s_curves = [
        ((144.0f32, 52.0f32), (138.6, 43.8), (120.0, 48.0), (100.0, 62.0)),
        ((100.0, 62.0), (80.0, 76.0), (50.0, 95.2), (50.0, 109.0)),
        ((50.0, 109.0), (50.0, 121.6), (74.0, 118.0), (100.0, 121.0)),
        ((100.0, 121.0), (124.0, 124.0), (152.0, 123.8), (152.0, 136.4)),
        ((152.0, 136.4), (152.0, 150.2), (126.0, 168.0), (100.0, 192.0)),
    ];
    let mut s_pts = Vec::new();
    for (idx, &(p0, p1, p2, p3)) in s_curves.iter().enumerate() {
        let mut pts = cubic(p0, p1, p2, p3, 32);
        if idx > 0 {
            pts.remove(0);
        }
        s_pts.extend(pts);
    }

    let mut bg = to_segs(&stem);
    bg.extend(to_segs(&conn));
    let fg = to_segs(&s_pts);
    bg.extend(fg.iter().copied());
    (bg, fg)
}

fn min_dist_segs(px: f32, py: f32, segs: &[Seg2D]) -> f32 {
    let mut best_sq = f32::MAX;
    for s in segs {
        let t = if s.len_sq > 1e-10 {
            (((px - s.ax) * s.dx + (py - s.ay) * s.dy) / s.len_sq).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let cx = s.ax + t * s.dx;
        let cy = s.ay + t * s.dy;
        let ddx = px - cx;
        let ddy = py - cy;
        let d2 = ddx * ddx + ddy * ddy;
        if d2 < best_sq {
            best_sq = d2;
        }
    }
    best_sq.sqrt()
}

pub fn get_cached_logo_rgba(size: u32) -> Vec<u8> {
    let lock = LOGO_CACHE.get_or_init(|| Mutex::new(Vec::new()));
    if let Ok(guard) = lock.lock() {
        if let Some((_, buf)) = guard.iter().find(|(sz, _)| *sz == size) {
            return buf.clone();
        }
    }
    let rendered = render_logo_rgba(size);
    if let Ok(mut guard) = lock.lock() {
        if guard.len() < 8 {
            guard.push((size, rendered.clone()));
        }
    }
    rendered
}

/// Renders `assets/smartblink-logo.svg` (`viewBox="10 20 190 200"`, `stroke-width="12"`,
/// `#8b95a5` at 50% opacity, `#00e5ff` foreground S-curve + `feDropShadow stdDeviation="3.5"`).
pub fn render_logo_rgba(size: u32) -> Vec<u8> {
    let (bg_segs, fg_segs) = LOGO_GEOM.get_or_init(build_logo_geom);
    let side = size.max(16) as usize;
    let mut out = vec![0u8; side * side * 4];

    let vb_x = 10.0f32;
    let vb_y = 20.0f32;
    let vb_w = 190.0f32;
    let vb_h = 200.0f32;
    let scale = (side as f32 / vb_w).min(side as f32 / vb_h);
    let ox = (side as f32 - vb_w * scale) * 0.5 - vb_x * scale;
    let oy = (side as f32 - vb_h * scale) * 0.5 - vb_y * scale;
    let aa = 1.15 / scale;
    let r_stroke = 6.0f32; // stroke-width="12" -> radius 6.0

    for y in 0..side {
        let vy = (y as f32 + 0.5 - oy) / scale;
        for x in 0..side {
            let vx = (x as f32 + 0.5 - ox) / scale;
            let d_bg = min_dist_segs(vx, vy, bg_segs);
            let d_fg = min_dist_segs(vx, vy, fg_segs);

            let cov = |d: f32, r: f32| -> f32 {
                let h = 0.5 * aa;
                if d <= r - h {
                    1.0
                } else if d >= r + h {
                    0.0
                } else {
                    let t = (d - (r - h)) / aa;
                    1.0 - t * t * (3.0 - 2.0 * t)
                }
            };

            // Group 1: #8b95a5 at opacity 0.5
            let a_bg = cov(d_bg, r_stroke) * 0.50;
            let mut r = (0x8B as f32 / 255.0) * a_bg;
            let mut g = (0x95 as f32 / 255.0) * a_bg;
            let mut b = (0xA5 as f32 / 255.0) * a_bg;
            let mut a = a_bg;

            // Drop shadow glow on foreground path: stdDeviation=3.5, flood=#00e5ff, opacity=0.65
            let glow_excess = (d_fg - r_stroke * 0.65).max(0.0);
            let a_glow = 0.62 * (-0.5 * (glow_excess / 4.2).powi(2)).exp();
            // Foreground stroke: #00e5ff
            let a_fg_stroke = cov(d_fg, r_stroke);
            let a_fg = (a_fg_stroke + a_glow * (1.0 - a_fg_stroke)).clamp(0.0, 1.0);

            if a_fg > 0.0 {
                let fg_r = 0.0f32;
                let fg_g = 0xE5 as f32 / 255.0;
                let fg_b = 1.0f32;
                r = fg_r * a_fg + r * (1.0 - a_fg);
                g = fg_g * a_fg + g * (1.0 - a_fg);
                b = fg_b * a_fg + b * (1.0 - a_fg);
                a = a_fg + a * (1.0 - a_fg);
            }

            if a > 1e-5 {
                let ur = ((r / a).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                let ug = ((g / a).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                let ub = ((b / a).clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                let ua = (a.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
                let idx = (y * side + x) * 4;
                out[idx] = ur;
                out[idx + 1] = ug;
                out[idx + 2] = ub;
                out[idx + 3] = ua;
            }
        }
    }
    out
}

unsafe fn window_dpi_scale(hwnd: HWND) -> f32 {
    let hdc = GetDC(hwnd);
    if hdc.0.is_null() {
        return 1.0;
    }
    let dpi = GetDeviceCaps(hdc, LOGPIXELSX);
    let _ = ReleaseDC(hwnd, hdc);
    if dpi > 0 {
        (dpi as f32 / 96.0).clamp(1.0, 3.0)
    } else {
        1.0
    }
}

unsafe fn apply_dwm_bento_frame(hwnd: HWND) {
    type DwmSetWindowAttributeFn =
        unsafe extern "system" fn(HWND, u32, *const std::ffi::c_void, u32) -> i32;
    let Ok(hmod) = LoadLibraryW(w!("dwmapi.dll")) else {
        return;
    };
    if let Some(proc) = GetProcAddress(hmod, s!("DwmSetWindowAttribute")) {
        let set_attr: DwmSetWindowAttributeFn = std::mem::transmute(proc);
        let dark: i32 = 1;
        let round: i32 = 2; // DWMWCP_ROUND
        let border_col: u32 = rgb_ref(COL_BORDER).0;
        let caption_col: u32 = rgb_ref(COL_CANVAS).0;
        let text_col: u32 = rgb_ref(COL_TEXT).0;
        let _ = set_attr(hwnd, 20, &dark as *const _ as _, 4); // DWMWA_USE_IMMERSIVE_DARK_MODE
        let _ = set_attr(hwnd, 33, &round as *const _ as _, 4); // DWMWA_WINDOW_CORNER_PREFERENCE
        let _ = set_attr(hwnd, 34, &border_col as *const _ as _, 4); // DWMWA_BORDER_COLOR
        let _ = set_attr(hwnd, 35, &caption_col as *const _ as _, 4); // DWMWA_CAPTION_COLOR
        let _ = set_attr(hwnd, 36, &text_col as *const _ as _, 4); // DWMWA_TEXT_COLOR
    }
}

unsafe fn load_app_icon(big: bool) -> HICON {
    let Ok(hmod) = GetModuleHandleW(None) else {
        return HICON(std::ptr::null_mut());
    };
    let hinst = HINSTANCE(hmod.0 as _);
    let sz = GetSystemMetrics(if big { SM_CXICON } else { SM_CXSMICON });
    if let Ok(h) = LoadImageW(
        hinst,
        PCWSTR(IDI_MAIN_RES as *const u16),
        IMAGE_ICON,
        sz,
        sz,
        LR_DEFAULTCOLOR,
    ) {
        if !h.is_invalid() {
            return HICON(h.0);
        }
    }
    HICON(std::ptr::null_mut())
}

pub fn set_tray_owner_hwnd(hwnd: HWND) {
    TRAY_OWNER_HWND.store(hwnd.0 as isize, Ordering::Release);
}

/// Refresh any open SmartBlink UI windows (Settings, About, Tray Flyout).
pub fn refresh_if_open() {
    unsafe {
        for slot in [&SETTINGS_HWND, &ABOUT_HWND, &FLYOUT_HWND] {
            let hwnd = HWND(slot.load(Ordering::Acquire) as *mut _);
            if !hwnd.0.is_null() && IsWindow(hwnd).as_bool() {
                let _ = InvalidateRect(hwnd, None, false);
            }
        }
    }
}

#[cfg(test)]
pub fn next_notify_mode(mode: NotifyMode) -> NotifyMode {
    match mode {
        NotifyMode::Both => NotifyMode::Sound,
        NotifyMode::Sound => NotifyMode::Led,
        NotifyMode::Led => NotifyMode::Silent,
        NotifyMode::Silent => NotifyMode::Both,
    }
}

#[cfg(test)]
pub fn next_led_mode(mode: LedMode) -> LedMode {
    match mode {
        LedMode::ScrollLock => LedMode::CapsLockGuarded,
        LedMode::CapsLockGuarded => LedMode::ScrollLock,
    }
}

#[cfg(test)]
pub fn next_fix_trigger(trigger: FixTrigger) -> FixTrigger {
    match trigger {
        FixTrigger::DoubleLeftShift => FixTrigger::DoubleLeftCtrl,
        FixTrigger::DoubleLeftCtrl => FixTrigger::PauseBreak,
        FixTrigger::PauseBreak => FixTrigger::DoubleLeftShift,
    }
}

struct FontSet {
    hero: HFONT,
    title: HFONT,
    card_title: HFONT,
    body: HFONT,
    caption: HFONT,
    mono: HFONT,
}

impl FontSet {
    unsafe fn new(s: f32) -> Self {
        let make = |px: f32, weight: i32, face: PCWSTR| -> HFONT {
            CreateFontW(
                -(px * s).round() as i32,
                0,
                0,
                0,
                weight,
                0,
                0,
                0,
                DEFAULT_CHARSET.0 as u32,
                OUT_DEFAULT_PRECIS.0 as u32,
                CLIP_DEFAULT_PRECIS.0 as u32,
                CLEARTYPE_QUALITY.0 as u32,
                0,
                face,
            )
        };
        Self {
            hero: make(24.0, 700, w!("Segoe UI")),
            title: make(14.5, 600, w!("Segoe UI")),
            card_title: make(13.5, 600, w!("Segoe UI")),
            body: make(12.5, 400, w!("Segoe UI")),
            caption: make(11.5, 400, w!("Segoe UI")),
            mono: make(11.5, 600, w!("Consolas")),
        }
    }

    unsafe fn destroy(&self) {
        for f in [
            self.hero,
            self.title,
            self.card_title,
            self.body,
            self.caption,
            self.mono,
        ] {
            if !f.0.is_null() {
                let _ = DeleteObject(f);
            }
        }
    }
}

unsafe fn draw_text_str(
    hdc: HDC,
    font: HFONT,
    color: (u8, u8, u8),
    mut rect: RECT,
    text: &str,
    flags: u32,
) {
    let old = SelectObject(hdc, HGDIOBJ(font.0));
    let _ = SetBkMode(hdc, TRANSPARENT);
    let _ = SetTextColor(hdc, rgb_ref(color));
    let mut wide: Vec<u16> = text.encode_utf16().collect();
    let _ = DrawTextW(
        hdc,
        &mut wide,
        &mut rect,
        windows::Win32::Graphics::Gdi::DRAW_TEXT_FORMAT(flags | DT_NOPREFIX.0),
    );
    let _ = SelectObject(hdc, old);
}

#[inline]
const fn rect(x: i32, y: i32, w: i32, h: i32) -> RECT {
    RECT {
        left: x,
        top: y,
        right: x + w,
        bottom: y + h,
    }
}

#[inline]
fn pt_in_rect(r: RECT, x: i32, y: i32) -> bool {
    x >= r.left && x < r.right && y >= r.top && y < r.bottom
}

unsafe fn present_canvas_with_text<F: FnOnce(HDC, &FontSet)>(
    hwnd: HWND,
    canvas: &Canvas,
    s: f32,
    draw_text_cb: F,
) {
    let mut ps = PAINTSTRUCT::default();
    let hdc = BeginPaint(hwnd, &mut ps);
    if hdc.0.is_null() {
        return;
    }
    let mem_dc = CreateCompatibleDC(hdc);
    if !mem_dc.0.is_null() {
        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: canvas.w,
                biHeight: -canvas.h, // top-down DIB
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
        if let Ok(dib) = CreateDIBSection(mem_dc, &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
            if !dib.0.is_null() && !bits.is_null() {
                std::ptr::copy_nonoverlapping(
                    canvas.px.as_ptr() as *const u8,
                    bits as *mut u8,
                    canvas.px.len() * 4,
                );
                let old_bmp = SelectObject(mem_dc, HGDIOBJ(dib.0));
                let fonts = FontSet::new(s);
                draw_text_cb(mem_dc, &fonts);
                fonts.destroy();
                let _ = BitBlt(hdc, 0, 0, canvas.w, canvas.h, mem_dc, 0, 0, SRCCOPY);
                let _ = SelectObject(mem_dc, old_bmp);
                let _ = DeleteObject(HBITMAP(dib.0));
            }
        }
        let _ = DeleteDC(mem_dc);
    }
    let _ = EndPaint(hwnd, &ps);
}

unsafe fn execute_hit_action(hwnd: HWND, action: HitAction, is_flyout: bool) {
    match action {
        HitAction::TogglePause => {
            let paused = hooks::toggle_paused();
            let tray_hwnd = HWND(TRAY_OWNER_HWND.load(Ordering::Acquire) as *mut _);
            if !tray_hwnd.0.is_null() {
                crate::tray::sync_tray_tooltip(tray_hwnd, paused);
            }
            refresh_if_open();
        }
        HitAction::TriggerTestAlert => {
            let cfg = config::get_runtime_config();
            signal::trigger_test_alert(cfg.notify_mode, cfg.led_mode);
            TEST_PULSE_ACTIVE.store(true, Ordering::Release);
            let s_hwnd = HWND(SETTINGS_HWND.load(Ordering::Acquire) as *mut _);
            if !s_hwnd.0.is_null() && IsWindow(s_hwnd).as_bool() {
                let _ = SetTimer(s_hwnd, TIMER_TEST_PULSE, 1100, None);
            }
            refresh_if_open();
        }
        HitAction::SetNotify(mode) => {
            config::update_runtime_config(|c| c.notify_mode = mode);
            refresh_if_open();
        }
        HitAction::SetLed(mode) => {
            config::update_runtime_config(|c| c.led_mode = mode);
            refresh_if_open();
        }
        HitAction::SetFix(trig) => {
            config::update_runtime_config(|c| c.fix_trigger = trig);
            refresh_if_open();
        }
        HitAction::ToggleAutostart => {
            let next = !autostart::is_registry_autostart_enabled();
            if autostart::set_registry_autostart(next) {
                config::update_runtime_config(|c| c.autostart = next);
            }
            refresh_if_open();
        }
        HitAction::ToggleElevatedTask => {
            if is_flyout {
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
            if autostart::is_elevated_task_registered() {
                let _ = autostart::unregister_elevated_task_interactive(hwnd);
            } else {
                let _ = autostart::register_elevated_task_interactive(hwnd);
            }
            refresh_if_open();
        }
        HitAction::ToggleLastProcess => {
            if let Some(proc_name) = config::get_last_foreground_process() {
                config::update_runtime_config(|c| {
                    if let Some(pos) = c.blacklist.iter().position(|b| b == &proc_name) {
                        c.blacklist.remove(pos);
                    } else {
                        c.blacklist.push(proc_name);
                    }
                });
                refresh_if_open();
            }
        }
        HitAction::RemoveBlacklistIdx(idx) => {
            config::update_runtime_config(|c| {
                if idx < c.blacklist.len() {
                    c.blacklist.remove(idx);
                }
            });
            refresh_if_open();
        }
        HitAction::ResetBlacklistDefaults => {
            config::update_runtime_config(|c| {
                c.blacklist = Config::default().blacklist;
            });
            refresh_if_open();
        }
        HitAction::OpenConfigFile => {
            if is_flyout {
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
            let path = config::config_path();
            let path_w: Vec<u16> = path
                .to_string_lossy()
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            let _ = ShellExecuteW(
                hwnd,
                w!("open"),
                PCWSTR(path_w.as_ptr()),
                None,
                None,
                SW_SHOWNORMAL,
            );
        }
        HitAction::OpenAboutWindow => {
            if is_flyout {
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
            show_about_window();
        }
        HitAction::OpenSettingsWindow => {
            if is_flyout {
                let _ = ShowWindow(hwnd, SW_HIDE);
            }
            show_settings_window();
        }
        HitAction::CloseWindow => {
            if is_flyout {
                let _ = ShowWindow(hwnd, SW_HIDE);
            } else {
                let _ = DestroyWindow(hwnd);
            }
        }
        HitAction::OpenWebsite => {
            let _ = ShellExecuteW(
                hwnd,
                w!("open"),
                w!("https://vkvstudio.com"),
                None,
                None,
                SW_SHOWNORMAL,
            );
        }
        HitAction::ExitApp => {
            let _ = ShowWindow(hwnd, SW_HIDE);
            let tray_hwnd = HWND(TRAY_OWNER_HWND.load(Ordering::Acquire) as *mut _);
            if !tray_hwnd.0.is_null() {
                let _ = PostMessageW(tray_hwnd, WM_CLOSE, WPARAM(0), LPARAM(0));
            }
        }
    }
}

/// Open or bring to foreground the Canonical Bento Settings window (`SmartBlinkSettingsWnd`).
pub fn show_settings_window() {
    unsafe {
        let existing = HWND(SETTINGS_HWND.load(Ordering::Acquire) as *mut _);
        if !existing.0.is_null() && IsWindow(existing).as_bool() {
            let _ = InvalidateRect(existing, None, false);
            let _ = ShowWindow(existing, SW_SHOWNORMAL);
            let _ = SetForegroundWindow(existing);
            return;
        }

        let Ok(hmod) = GetModuleHandleW(None) else {
            return;
        };
        let hinst = HINSTANCE(hmod.0 as _);
        let cls = w!("SmartBlinkSettingsWnd");

        SETTINGS_CLASS_ONCE.call_once(|| {
            let wc = WNDCLASSW {
                lpfnWndProc: Some(settings_wnd_proc),
                hInstance: hinst,
                hIcon: load_app_icon(true),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                lpszClassName: cls,
                ..Default::default()
            };
            let _ = RegisterClassW(&wc);
        });

        let screen_hdc = GetDC(None);
        let dpi = if !screen_hdc.0.is_null() {
            let d = GetDeviceCaps(screen_hdc, LOGPIXELSX);
            let _ = ReleaseDC(None, screen_hdc);
            if d > 0 { d } else { 96 }
        } else {
            96
        };
        let s = (dpi as f32 / 96.0).clamp(1.0, 3.0);

        let client_w = (860.0 * s).round() as i32;
        let client_h = (608.0 * s).round() as i32;
        let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_VISIBLE;
        let mut wr = RECT {
            left: 0,
            top: 0,
            right: client_w,
            bottom: client_h,
        };
        let _ = AdjustWindowRectEx(&mut wr, style, false, WINDOW_EX_STYLE(0));
        let wnd_w = wr.right - wr.left;
        let wnd_h = wr.bottom - wr.top;

        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let screen_h = GetSystemMetrics(SM_CYSCREEN);
        let pos_x = ((screen_w - wnd_w) / 2).max(24);
        let pos_y = ((screen_h - wnd_h) / 2).max(24);

        if let Ok(hwnd) = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            cls,
            w!("SmartBlink — Settings"),
            style,
            pos_x,
            pos_y,
            wnd_w,
            wnd_h,
            None,
            HMENU(std::ptr::null_mut()),
            hinst,
            None,
        ) {
            SETTINGS_HWND.store(hwnd.0 as isize, Ordering::Release);
            apply_dwm_bento_frame(hwnd);
            let _ = ShowWindow(hwnd, SW_SHOWNORMAL);
            let _ = SetForegroundWindow(hwnd);
        }
    }
}

/// Open or bring to foreground the Canonical Bento About window (`SmartBlinkAboutWnd`).
pub fn show_about_window() {
    unsafe {
        let existing = HWND(ABOUT_HWND.load(Ordering::Acquire) as *mut _);
        if !existing.0.is_null() && IsWindow(existing).as_bool() {
            let _ = InvalidateRect(existing, None, false);
            let _ = ShowWindow(existing, SW_SHOWNORMAL);
            let _ = SetForegroundWindow(existing);
            return;
        }

        let Ok(hmod) = GetModuleHandleW(None) else {
            return;
        };
        let hinst = HINSTANCE(hmod.0 as _);
        let cls = w!("SmartBlinkAboutWnd");

        ABOUT_CLASS_ONCE.call_once(|| {
            let wc = WNDCLASSW {
                lpfnWndProc: Some(about_wnd_proc),
                hInstance: hinst,
                hIcon: load_app_icon(true),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                lpszClassName: cls,
                ..Default::default()
            };
            let _ = RegisterClassW(&wc);
        });

        let screen_hdc = GetDC(None);
        let dpi = if !screen_hdc.0.is_null() {
            let d = GetDeviceCaps(screen_hdc, LOGPIXELSX);
            let _ = ReleaseDC(None, screen_hdc);
            if d > 0 { d } else { 96 }
        } else {
            96
        };
        let s = (dpi as f32 / 96.0).clamp(1.0, 3.0);

        let client_w = (540.0 * s).round() as i32;
        let client_h = (392.0 * s).round() as i32;
        let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_VISIBLE;
        let mut wr = RECT {
            left: 0,
            top: 0,
            right: client_w,
            bottom: client_h,
        };
        let _ = AdjustWindowRectEx(&mut wr, style, false, WINDOW_EX_STYLE(0));
        let wnd_w = wr.right - wr.left;
        let wnd_h = wr.bottom - wr.top;

        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let screen_h = GetSystemMetrics(SM_CYSCREEN);
        let pos_x = ((screen_w - wnd_w) / 2).max(24);
        let pos_y = ((screen_h - wnd_h) / 2).max(24);

        if let Ok(hwnd) = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            cls,
            w!("About SmartBlink"),
            style,
            pos_x,
            pos_y,
            wnd_w,
            wnd_h,
            None,
            HMENU(std::ptr::null_mut()),
            hinst,
            None,
        ) {
            ABOUT_HWND.store(hwnd.0 as isize, Ordering::Release);
            apply_dwm_bento_frame(hwnd);
            let _ = ShowWindow(hwnd, SW_SHOWNORMAL);
            let _ = SetForegroundWindow(hwnd);
        }
    }
}

/// Show the borderless Dark Bento Tray Flyout right near the taskbar / cursor.
pub fn show_tray_flyout(tray_hwnd: HWND) {
    unsafe {
        set_tray_owner_hwnd(tray_hwnd);
        let existing = HWND(FLYOUT_HWND.load(Ordering::Acquire) as *mut _);
        if !existing.0.is_null() && IsWindow(existing).as_bool() {
            let _ = DestroyWindow(existing);
            FLYOUT_HWND.store(0, Ordering::Release);
        }

        let Ok(hmod) = GetModuleHandleW(None) else {
            return;
        };
        let hinst = HINSTANCE(hmod.0 as _);
        let cls = w!("SmartBlinkTrayPopupWnd");

        FLYOUT_CLASS_ONCE.call_once(|| {
            let wc = WNDCLASSW {
                lpfnWndProc: Some(flyout_wnd_proc),
                hInstance: hinst,
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                lpszClassName: cls,
                ..Default::default()
            };
            let _ = RegisterClassW(&wc);
        });

        let screen_hdc = GetDC(None);
        let dpi = if !screen_hdc.0.is_null() {
            let d = GetDeviceCaps(screen_hdc, LOGPIXELSX);
            let _ = ReleaseDC(None, screen_hdc);
            if d > 0 { d } else { 96 }
        } else {
            96
        };
        let s = (dpi as f32 / 96.0).clamp(1.0, 3.0);

        let wnd_w = (368.0 * s).round() as i32;
        let wnd_h = (472.0 * s).round() as i32;

        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);

        let mut work = RECT {
            left: 0,
            top: 0,
            right: GetSystemMetrics(SM_CXSCREEN),
            bottom: GetSystemMetrics(SM_CYSCREEN),
        };
        let _ = SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            Some(&mut work as *mut _ as *mut _),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        );

        let margin = (10.0 * s).round() as i32;
        let pos_x = (pt.x - wnd_w / 2)
            .max(work.left + margin)
            .min(work.right - wnd_w - margin);
        let pos_y = (pt.y - wnd_h - margin)
            .max(work.top + margin)
            .min(work.bottom - wnd_h - margin);

        if let Ok(hwnd) = CreateWindowExW(
            WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            cls,
            w!("SmartBlink Tray Menu"),
            WS_POPUP | WS_VISIBLE,
            pos_x,
            pos_y,
            wnd_w,
            wnd_h,
            None,
            HMENU(std::ptr::null_mut()),
            hinst,
            None,
        ) {
            FLYOUT_HWND.store(hwnd.0 as isize, Ordering::Release);
            apply_dwm_bento_frame(hwnd);
            let _ = ShowWindow(hwnd, SW_SHOW);
            let _ = SetForegroundWindow(hwnd);
        }
    }
}

unsafe fn handle_mouse_move(hwnd: HWND, hover_slot: &AtomicIsize, zones: &[HitZone], lparam: LPARAM) {
    let mx = (lparam.0 & 0xFFFF) as i16 as i32;
    let my = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
    let mut tme = TRACKMOUSEEVENT {
        cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
        dwFlags: TME_LEAVE,
        hwndTrack: hwnd,
        dwHoverTime: 0,
    };
    let _ = TrackMouseEvent(&mut tme);

    let new_hover = zones
        .iter()
        .find(|z| pt_in_rect(z.rect, mx, my))
        .map(|z| z.id)
        .unwrap_or(-1);
    let prev = hover_slot.swap(new_hover, Ordering::AcqRel);
    if prev != new_hover {
        let _ = InvalidateRect(hwnd, None, false);
    }
}

unsafe fn handle_set_cursor(hover_slot: &AtomicIsize) -> Option<LRESULT> {
    if hover_slot.load(Ordering::Acquire) >= 0 {
        if let Ok(cur) = LoadCursorW(None, IDC_HAND) {
            let _ = SetCursor(cur);
            return Some(LRESULT(1));
        }
    }
    None
}

unsafe extern "system" fn settings_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            SETTINGS_HOVER.store(-1, Ordering::Release);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_MOUSEMOVE => {
            let s = window_dpi_scale(hwnd);
            let cfg = config::get_runtime_config();
            let zones = build_settings_zones(s, &cfg);
            handle_mouse_move(hwnd, &SETTINGS_HOVER, &zones, lparam);
            LRESULT(0)
        }
        WM_MOUSELEAVE_MSG => {
            if SETTINGS_HOVER.swap(-1, Ordering::AcqRel) != -1 {
                let _ = InvalidateRect(hwnd, None, false);
            }
            LRESULT(0)
        }
        WM_SETCURSOR => {
            if let Some(res) = handle_set_cursor(&SETTINGS_HOVER) {
                return res;
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_LBUTTONDOWN | WM_LBUTTONUP => {
            if msg == WM_LBUTTONUP {
                let mx = (lparam.0 & 0xFFFF) as i16 as i32;
                let my = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
                let s = window_dpi_scale(hwnd);
                let cfg = config::get_runtime_config();
                let zones = build_settings_zones(s, &cfg);
                if let Some(hit) = zones.iter().find(|z| pt_in_rect(z.rect, mx, my)) {
                    execute_hit_action(hwnd, hit.action, false);
                }
            }
            LRESULT(0)
        }
        WM_TIMER => {
            if wparam.0 == TIMER_TEST_PULSE {
                let _ = KillTimer(hwnd, TIMER_TEST_PULSE);
                TEST_PULSE_ACTIVE.store(false, Ordering::Release);
                let _ = InvalidateRect(hwnd, None, false);
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if wparam.0 == VK_ESCAPE.0 as usize {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_PAINT => {
            paint_settings_window(hwnd);
            LRESULT(0)
        }
        WM_CLOSE => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            SETTINGS_HWND.store(0, Ordering::Release);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe extern "system" fn about_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            ABOUT_HOVER.store(-1, Ordering::Release);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_MOUSEMOVE => {
            let s = window_dpi_scale(hwnd);
            let zones = build_about_zones(s);
            handle_mouse_move(hwnd, &ABOUT_HOVER, &zones, lparam);
            LRESULT(0)
        }
        WM_MOUSELEAVE_MSG => {
            if ABOUT_HOVER.swap(-1, Ordering::AcqRel) != -1 {
                let _ = InvalidateRect(hwnd, None, false);
            }
            LRESULT(0)
        }
        WM_SETCURSOR => {
            if let Some(res) = handle_set_cursor(&ABOUT_HOVER) {
                return res;
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_LBUTTONUP => {
            let mx = (lparam.0 & 0xFFFF) as i16 as i32;
            let my = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
            let s = window_dpi_scale(hwnd);
            let zones = build_about_zones(s);
            if let Some(hit) = zones.iter().find(|z| pt_in_rect(z.rect, mx, my)) {
                execute_hit_action(hwnd, hit.action, false);
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if wparam.0 == VK_ESCAPE.0 as usize {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_PAINT => {
            paint_about_window(hwnd);
            LRESULT(0)
        }
        WM_CLOSE => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            ABOUT_HWND.store(0, Ordering::Release);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe extern "system" fn flyout_wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_CREATE => {
            FLYOUT_HOVER.store(-1, Ordering::Release);
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_ACTIVATE => {
            if (wparam.0 & 0xFFFF) as u32 == WA_INACTIVE {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_MOUSEMOVE => {
            let s = window_dpi_scale(hwnd);
            let cfg = config::get_runtime_config();
            let zones = build_flyout_zones(s, &cfg);
            handle_mouse_move(hwnd, &FLYOUT_HOVER, &zones, lparam);
            LRESULT(0)
        }
        WM_MOUSELEAVE_MSG => {
            if FLYOUT_HOVER.swap(-1, Ordering::AcqRel) != -1 {
                let _ = InvalidateRect(hwnd, None, false);
            }
            LRESULT(0)
        }
        WM_SETCURSOR => {
            if let Some(res) = handle_set_cursor(&FLYOUT_HOVER) {
                return res;
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_LBUTTONUP => {
            let mx = (lparam.0 & 0xFFFF) as i16 as i32;
            let my = ((lparam.0 >> 16) & 0xFFFF) as i16 as i32;
            let s = window_dpi_scale(hwnd);
            let cfg = config::get_runtime_config();
            let zones = build_flyout_zones(s, &cfg);
            if let Some(hit) = zones.iter().find(|z| pt_in_rect(z.rect, mx, my)) {
                execute_hit_action(hwnd, hit.action, true);
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if wparam.0 == VK_ESCAPE.0 as usize {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_PAINT => {
            paint_flyout_window(hwnd);
            LRESULT(0)
        }
        WM_CLOSE => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            FLYOUT_HWND.store(0, Ordering::Release);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

#[inline]
fn sc(v: i32, s: f32) -> i32 {
    (v as f32 * s).round() as i32
}

#[inline]
fn srect(x: i32, y: i32, w: i32, h: i32, s: f32) -> RECT {
    rect(sc(x, s), sc(y, s), sc(w, s), sc(h, s))
}

/// Build interactive hit zones for the 12-column Bento Settings window (`860 x 608` logical px).
pub fn build_settings_zones(s: f32, cfg: &Config) -> Vec<HitZone> {
    let mut zones = Vec::with_capacity(20);
    // 1. Hero Engine Toggle pill (inside Tile 1: x=20, y=20, w=478, h=144)
    zones.push(HitZone {
        id: 1,
        rect: srect(368, 34, 114, 32, s),
        action: HitAction::TogglePause,
    });
    // 2. Test Alert CTA button (inside Tile 2: x=512, y=20, w=328, h=144)
    zones.push(HitZone {
        id: 2,
        rect: srect(530, 104, 292, 44, s),
        action: HitAction::TriggerTestAlert,
    });
    // 3..6. Alert Mode 2x2 sub-cards (inside Tile 3: x=20, y=178, w=478, h=168)
    let notify_items = [
        (3, 36, 224, 216, 52, NotifyMode::Both),
        (4, 264, 224, 218, 52, NotifyMode::Sound),
        (5, 36, 284, 216, 52, NotifyMode::Led),
        (6, 264, 284, 218, 52, NotifyMode::Silent),
    ];
    for &(id, x, y, w, h, mode) in &notify_items {
        zones.push(HitZone {
            id,
            rect: srect(x, y, w, h, s),
            action: HitAction::SetNotify(mode),
        });
    }
    // 7..8. LED Target 2x1 sub-cards (inside Tile 4: x=512, y=178, w=328, h=168)
    zones.push(HitZone {
        id: 7,
        rect: srect(528, 224, 296, 52, s),
        action: HitAction::SetLed(LedMode::ScrollLock),
    });
    zones.push(HitZone {
        id: 8,
        rect: srect(528, 284, 296, 52, s),
        action: HitAction::SetLed(LedMode::CapsLockGuarded),
    });
    // 9..11. Fix Gesture 3 keycap sub-cards (inside Tile 5: x=20, y=360, w=403, h=140)
    let fix_items = [
        (9, 36, 408, 118, 78, FixTrigger::DoubleLeftShift),
        (10, 162, 408, 118, 78, FixTrigger::DoubleLeftCtrl),
        (11, 288, 408, 119, 78, FixTrigger::PauseBreak),
    ];
    for &(id, x, y, w, h, trig) in &fix_items {
        zones.push(HitZone {
            id,
            rect: srect(x, y, w, h, s),
            action: HitAction::SetFix(trig),
        });
    }
    // 12..13. System Startup & Admin Task toggle rows (inside Tile 6: x=437, y=360, w=403, h=140)
    zones.push(HitZone {
        id: 12,
        rect: srect(453, 406, 371, 38, s),
        action: HitAction::ToggleAutostart,
    });
    zones.push(HitZone {
        id: 13,
        rect: srect(453, 450, 371, 38, s),
        action: HitAction::ToggleElevatedTask,
    });
    // 14.. Immune Apps chips (inside Tile 7: x=20, y=514, w=548, h=74)
    if config::get_last_foreground_process().is_some() {
        zones.push(HitZone {
            id: 14,
            rect: srect(396, 522, 158, 24, s),
            action: HitAction::ToggleLastProcess,
        });
    } else {
        zones.push(HitZone {
            id: 14,
            rect: srect(436, 522, 118, 24, s),
            action: HitAction::ResetBlacklistDefaults,
        });
    }
    let mut chip_x = 36;
    for (idx, app) in cfg.blacklist.iter().take(6).enumerate() {
        let chip_w = ((app.len() as i32 * 7) + 28).clamp(62, 118);
        if chip_x + chip_w <= 554 {
            zones.push(HitZone {
                id: 100 + idx as isize,
                rect: srect(chip_x, 554, chip_w, 24, s),
                action: HitAction::RemoveBlacklistIdx(idx),
            });
            chip_x += chip_w + 6;
        }
    }
    // 15..16. Quick Actions (inside Tile 8: x=582, y=514, w=258, h=74)
    zones.push(HitZone {
        id: 15,
        rect: srect(596, 530, 112, 42, s),
        action: HitAction::OpenConfigFile,
    });
    zones.push(HitZone {
        id: 16,
        rect: srect(714, 530, 112, 42, s),
        action: HitAction::OpenAboutWindow,
    });
    zones
}

unsafe fn paint_settings_window(hwnd: HWND) {
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);
    let w = (rc.right - rc.left).max(1);
    let h = (rc.bottom - rc.top).max(1);
    let s = window_dpi_scale(hwnd);
    let cfg = config::get_runtime_config();
    let paused = hooks::is_paused();
    let elevated = autostart::is_process_elevated();
    let run_on = autostart::is_registry_autostart_enabled();
    let task_on = autostart::is_elevated_task_registered();
    let pulse = TEST_PULSE_ACTIVE.load(Ordering::Acquire);
    let hov = SETTINGS_HOVER.load(Ordering::Acquire);
    let last_proc = config::get_last_foreground_process();

    let mut c = Canvas::new(w, h, COL_CANVAS);
    let rad = 16.0 * s;
    let sub_rad = 10.0 * s;

    // 1. Tile 1: Hero Brand + Live Engine (col-span-7: x=20, y=20, w=478, h=144)
    let t1 = srect(20, 20, 478, 144, s);
    c.fill_round_rect(t1, rad, COL_TILE_HERO, 1.0);
    c.fill_radial_glow_clipped(t1, rad, 84.0 * s, 92.0 * s, 140.0 * s, COL_CYAN, 0.16);
    c.stroke_round_rect(t1, rad, 1.0 * s, COL_BORDER_HERO, 1.0);
    c.blit_logo(sc(34, s), sc(40, s), sc(104, s));

    let eng_pill = srect(368, 34, 114, 32, s);
    let eng_col = if hov == 1 { COL_SUB_HOVER } else { COL_SUB };
    c.fill_round_rect(eng_pill, 16.0 * s, eng_col, 1.0);
    c.stroke_round_rect(
        eng_pill,
        16.0 * s,
        1.0 * s,
        if paused { COL_AMBER } else { COL_CYAN },
        0.65,
    );
    c.fill_circle(
        384.0 * s,
        50.0 * s,
        4.5 * s,
        if paused { COL_AMBER } else { COL_CYAN },
        1.0,
    );

    // 2. Tile 2: Live Hardware Alert Tester (col-span-5: x=512, y=20, w=328, h=144)
    let t2 = srect(512, 20, 328, 144, s);
    c.fill_round_rect(t2, rad, COL_TILE, 1.0);
    if pulse {
        c.fill_radial_glow_clipped(t2, rad, 676.0 * s, 92.0 * s, 150.0 * s, COL_CYAN, 0.25);
    }
    c.stroke_round_rect(t2, rad, 1.0 * s, if pulse { COL_CYAN } else { COL_BORDER }, 1.0);
    let btn_test = srect(530, 104, 292, 44, s);
    let btn_col = if hov == 2 { COL_CYAN_HOVER } else { COL_CYAN };
    c.fill_round_rect(btn_test, 11.0 * s, btn_col, 1.0);

    // 3. Tile 3: Alert Channels 2x2 Grid (col-span-7: x=20, y=178, w=478, h=168)
    let t3 = srect(20, 178, 478, 168, s);
    c.fill_round_rect(t3, rad, COL_TILE, 1.0);
    c.stroke_round_rect(t3, rad, 1.0 * s, COL_BORDER, 1.0);
    let notify_cards = [
        (3isize, 36, 224, 216, 52, NotifyMode::Both),
        (4, 264, 224, 218, 52, NotifyMode::Sound),
        (5, 36, 284, 216, 52, NotifyMode::Led),
        (6, 264, 284, 218, 52, NotifyMode::Silent),
    ];
    for &(id, x, y, cw, ch, mode) in &notify_cards {
        let cr = srect(x, y, cw, ch, s);
        let active = cfg.notify_mode == mode;
        let fill = if active {
            COL_SUB_ACTIVE
        } else if hov == id {
            COL_SUB_HOVER
        } else {
            COL_SUB
        };
        c.fill_round_rect(cr, sub_rad, fill, 1.0);
        c.stroke_round_rect(
            cr,
            sub_rad,
            if active { 1.4 * s } else { 1.0 * s },
            if active { COL_CYAN } else { COL_BORDER },
            1.0,
        );
        c.fill_circle(
            (x + 18) as f32 * s,
            (y + 26) as f32 * s,
            4.5 * s,
            if active { COL_CYAN } else { COL_TEXT_MUTED },
            1.0,
        );
    }

    // 4. Tile 4: Keyboard LED Target (col-span-5: x=512, y=178, w=328, h=168)
    let t4 = srect(512, 178, 328, 168, s);
    c.fill_round_rect(t4, rad, COL_TILE, 1.0);
    c.stroke_round_rect(t4, rad, 1.0 * s, COL_BORDER, 1.0);
    for &(id, y, mode) in &[
        (7isize, 224, LedMode::ScrollLock),
        (8, 284, LedMode::CapsLockGuarded),
    ] {
        let cr = srect(528, y, 296, 52, s);
        let active = cfg.led_mode == mode;
        let fill = if active {
            COL_SUB_ACTIVE
        } else if hov == id {
            COL_SUB_HOVER
        } else {
            COL_SUB
        };
        c.fill_round_rect(cr, sub_rad, fill, 1.0);
        c.stroke_round_rect(
            cr,
            sub_rad,
            if active { 1.4 * s } else { 1.0 * s },
            if active { COL_CYAN } else { COL_BORDER },
            1.0,
        );
        c.fill_circle(
            546.0 * s,
            (y + 26) as f32 * s,
            4.5 * s,
            if active { COL_CYAN } else { COL_TEXT_MUTED },
            1.0,
        );
    }

    // 5. Tile 5: Fix Gesture Keycaps (col-span-6: x=20, y=360, w=403, h=140)
    let t5 = srect(20, 360, 403, 140, s);
    c.fill_round_rect(t5, rad, COL_TILE, 1.0);
    c.stroke_round_rect(t5, rad, 1.0 * s, COL_BORDER, 1.0);
    for &(id, x, w_k, trig) in &[
        (9isize, 36, 118, FixTrigger::DoubleLeftShift),
        (10, 162, 118, FixTrigger::DoubleLeftCtrl),
        (11, 288, 119, FixTrigger::PauseBreak),
    ] {
        let cr = srect(x, 408, w_k, 78, s);
        let active = cfg.fix_trigger == trig;
        let fill = if active {
            COL_SUB_ACTIVE
        } else if hov == id {
            COL_SUB_HOVER
        } else {
            COL_SUB
        };
        c.fill_round_rect(cr, sub_rad, fill, 1.0);
        c.stroke_round_rect(
            cr,
            sub_rad,
            if active { 1.4 * s } else { 1.0 * s },
            if active { COL_CYAN } else { COL_BORDER },
            1.0,
        );
        let cap = srect(x + 12, 418, w_k - 24, 30, s);
        c.fill_round_rect(cap, 6.0 * s, (22, 29, 43), 1.0);
        c.stroke_round_rect(cap, 6.0 * s, 1.0 * s, if active { COL_CYAN } else { COL_SLATE }, 0.7);
    }

    // 6. Tile 6: System Startup & Privilege (col-span-6: x=437, y=360, w=403, h=140)
    let t6 = srect(437, 360, 403, 140, s);
    c.fill_round_rect(t6, rad, COL_TILE, 1.0);
    c.stroke_round_rect(t6, rad, 1.0 * s, COL_BORDER, 1.0);
    for &(id, y, is_on) in &[(12isize, 406, run_on), (13, 450, task_on)] {
        let rr = srect(453, y, 371, 38, s);
        c.fill_round_rect(
            rr,
            9.0 * s,
            if hov == id { COL_SUB_HOVER } else { COL_SUB },
            1.0,
        );
        c.draw_toggle_switch(sc(812, s), sc(y + 19, s), s, is_on);
    }

    // 7. Tile 7: Immune Apps Bar (col-span-8: x=20, y=514, w=548, h=74)
    let t7 = srect(20, 514, 548, 74, s);
    c.fill_round_rect(t7, rad, COL_TILE_ALT, 1.0);
    c.stroke_round_rect(t7, rad, 1.0 * s, COL_BORDER, 1.0);
    let act_chip = if last_proc.is_some() {
        srect(396, 522, 158, 24, s)
    } else {
        srect(436, 522, 118, 24, s)
    };
    c.fill_round_rect(
        act_chip,
        12.0 * s,
        if hov == 14 { COL_SUB_HOVER } else { COL_SUB_ACTIVE },
        1.0,
    );
    c.stroke_round_rect(act_chip, 12.0 * s, 1.0 * s, COL_CYAN, 0.75);

    let mut chip_x = 36;
    for (idx, app) in cfg.blacklist.iter().take(6).enumerate() {
        let chip_w = ((app.len() as i32 * 7) + 28).clamp(62, 118);
        if chip_x + chip_w <= 554 {
            let cr = srect(chip_x, 554, chip_w, 24, s);
            let h_id = 100 + idx as isize;
            c.fill_round_rect(
                cr,
                8.0 * s,
                if hov == h_id { COL_RED_BG } else { COL_SUB },
                1.0,
            );
            c.stroke_round_rect(
                cr,
                8.0 * s,
                1.0 * s,
                if hov == h_id { COL_RED_BORDER } else { COL_BORDER },
                1.0,
            );
            chip_x += chip_w + 6;
        }
    }

    // 8. Tile 8: Quick Actions (col-span-4: x=582, y=514, w=258, h=74)
    let t8 = srect(582, 514, 258, 74, s);
    c.fill_round_rect(t8, rad, COL_TILE_ALT, 1.0);
    c.stroke_round_rect(t8, rad, 1.0 * s, COL_BORDER, 1.0);
    for &(id, x) in &[(15isize, 596), (16, 714)] {
        let br = srect(x, 530, 112, 42, s);
        c.fill_round_rect(br, 10.0 * s, if hov == id { COL_SUB_HOVER } else { COL_SUB }, 1.0);
        c.stroke_round_rect(br, 10.0 * s, 1.0 * s, COL_BORDER, 1.0);
    }

    present_canvas_with_text(hwnd, &c, s, |hdc, f| {
        // Tile 1 text
        draw_text_str(hdc, f.hero, COL_TEXT, srect(152, 34, 210, 32, s), "SmartBlink", DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0);
        draw_text_str(hdc, f.mono, COL_CYAN, srect(288, 38, 70, 24, s), "v1.0.0", DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0);
        draw_text_str(
            hdc,
            f.caption,
            if paused { COL_AMBER } else { COL_CYAN },
            srect(396, 34, 80, 32, s),
            if paused { "PAUSED" } else { "ACTIVE" },
            DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0,
        );
        draw_text_str(
            hdc,
            f.body,
            COL_TEXT_SUB,
            srect(152, 72, 330, 38, s),
            "Wrong-layout detector & one-gesture word fixer.\r\nZero auto-replace. 100% RAM-only ring buffer.",
            DT_LEFT.0 | DT_WORDBREAK.0,
        );
        draw_text_str(
            hdc,
            f.mono,
            COL_SLATE,
            srect(152, 122, 330, 22, s),
            "20 Languages  ·  <0.1 ms Hook  ·  Selection: Ctrl+Alt+Z",
            DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0,
        );

        // Tile 2 text
        draw_text_str(hdc, f.title, COL_TEXT, srect(530, 34, 290, 24, s), "Live Hardware Alert Test", DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0);
        draw_text_str(
            hdc,
            f.caption,
            COL_TEXT_SUB,
            srect(530, 60, 290, 36, s),
            "Fires your current WASAPI knock + keyboard LED pulse immediately.",
            DT_LEFT.0 | DT_WORDBREAK.0,
        );
        draw_text_str(
            hdc,
            f.card_title,
            COL_CANVAS,
            srect(530, 104, 292, 44, s),
            if pulse { "Pulse Sent — Check LED & Audio ✓" } else { "Trigger Test Alert Now" },
            DT_CENTER.0 | DT_VCENTER.0 | DT_SINGLELINE.0,
        );

        // Tile 3 text
        draw_text_str(hdc, f.title, COL_TEXT, srect(36, 192, 220, 24, s), "Alert Channels", DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0);
        draw_text_str(hdc, f.caption, COL_TEXT_MUTED, srect(250, 192, 232, 24, s), "Direct 1-click channel selection", DT_RIGHT.0 | DT_VCENTER.0 | DT_SINGLELINE.0);
        let notify_labels = [
            (36, 224, 216, "Both: Click + LED", "Full audio & peripheral cue"),
            (264, 224, 218, "Audio Click Only", "Low-passed WASAPI knock"),
            (36, 284, 216, "LED Blink Only", "Net-zero keyboard diode"),
            (264, 284, 218, "Silent Mode", "Tray state only, no signal"),
        ];
        for &(x, y, cw, t, sub) in &notify_labels {
            draw_text_str(hdc, f.card_title, COL_TEXT, srect(x + 32, y + 7, cw - 38, 20, s), t, DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0);
            draw_text_str(hdc, f.caption, COL_TEXT_SUB, srect(x + 32, y + 26, cw - 38, 18, s), sub, DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0);
        }

        // Tile 4 text
        draw_text_str(hdc, f.title, COL_TEXT, srect(528, 192, 200, 24, s), "LED Diode Target", DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0);
        draw_text_str(hdc, f.mono, COL_CYAN, srect(720, 192, 104, 24, s), "NET-ZERO", DT_RIGHT.0 | DT_VCENTER.0 | DT_SINGLELINE.0);
        draw_text_str(hdc, f.card_title, COL_TEXT, srect(560, 231, 256, 20, s), "Scroll Lock LED", DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0);
        draw_text_str(hdc, f.caption, COL_TEXT_SUB, srect(560, 250, 256, 18, s), "Desktop default · Never touches case", DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0);
        draw_text_str(hdc, f.card_title, COL_TEXT, srect(560, 291, 256, 20, s), "Caps Lock LED (Guarded)", DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0);
        draw_text_str(hdc, f.caption, COL_TEXT_SUB, srect(560, 310, 256, 18, s), "Laptops · Pulses after typing stops", DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0);

        // Tile 5 text
        draw_text_str(hdc, f.title, COL_TEXT, srect(36, 374, 240, 24, s), "One-Gesture Fix Trigger", DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0);
        draw_text_str(hdc, f.mono, COL_SLATE, srect(270, 374, 137, 24, s), "<= 300 ms tap", DT_RIGHT.0 | DT_VCENTER.0 | DT_SINGLELINE.0);
        for &(x, w_k, cap_txt, sub_txt) in &[
            (36, 118, "2x L-Shift", "Recommended"),
            (162, 118, "2x L-Ctrl", "Alternative"),
            (288, 119, "Pause Key", "Single tap"),
        ] {
            draw_text_str(hdc, f.mono, COL_TEXT, srect(x + 12, 418, w_k - 24, 30, s), cap_txt, DT_CENTER.0 | DT_VCENTER.0 | DT_SINGLELINE.0);
            draw_text_str(hdc, f.caption, COL_TEXT_SUB, srect(x + 6, 456, w_k - 12, 20, s), sub_txt, DT_CENTER.0 | DT_VCENTER.0 | DT_SINGLELINE.0);
        }

        // Tile 6 text
        draw_text_str(hdc, f.title, COL_TEXT, srect(453, 374, 230, 24, s), "Startup & Privilege", DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0);
        draw_text_str(
            hdc,
            f.mono,
            if elevated { COL_CYAN } else { COL_SLATE },
            srect(670, 374, 154, 24, s),
            if elevated { "ADMIN / UIPI" } else { "STANDARD USER" },
            DT_RIGHT.0 | DT_VCENTER.0 | DT_SINGLELINE.0,
        );
        draw_text_str(hdc, f.card_title, COL_TEXT, srect(467, 406, 285, 38, s), "Launch at Windows Startup (HKCU)", DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0);
        draw_text_str(hdc, f.card_title, COL_TEXT, srect(467, 450, 285, 38, s), "Work Above Admin Apps (ONLOGON)", DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0);

        // Tile 7 text
        let imm_hdr = format!("Immune Apps ({}) — click chip to remove", cfg.blacklist.len());
        draw_text_str(hdc, f.title, COL_TEXT, srect(36, 522, 350, 24, s), &imm_hdr, DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0);
        let proc_btn_label = if let Some(ref p) = last_proc {
            if cfg.blacklist.iter().any(|b| b == p) {
                format!("✓ {p}.exe")
            } else {
                format!("+ Exclude {p}")
            }
        } else {
            "Reset Defaults".to_string()
        };
        let act_rect = if last_proc.is_some() {
            srect(396, 522, 158, 24, s)
        } else {
            srect(436, 522, 118, 24, s)
        };
        draw_text_str(hdc, f.caption, COL_CYAN, act_rect, &proc_btn_label, DT_CENTER.0 | DT_VCENTER.0 | DT_SINGLELINE.0 | DT_END_ELLIPSIS.0);

        let mut cx_chip = 36;
        for app in cfg.blacklist.iter().take(6) {
            let cw = ((app.len() as i32 * 7) + 28).clamp(62, 118);
            if cx_chip + cw <= 554 {
                let lbl = format!("{app} ×");
                draw_text_str(hdc, f.mono, COL_TEXT_SUB, srect(cx_chip + 6, 554, cw - 12, 24, s), &lbl, DT_CENTER.0 | DT_VCENTER.0 | DT_SINGLELINE.0 | DT_END_ELLIPSIS.0);
                cx_chip += cw + 6;
            }
        }

        // Tile 8 text
        draw_text_str(hdc, f.card_title, COL_TEXT, srect(596, 530, 112, 42, s), "config.toml ↗", DT_CENTER.0 | DT_VCENTER.0 | DT_SINGLELINE.0);
        draw_text_str(hdc, f.card_title, COL_CYAN, srect(714, 530, 112, 42, s), "About", DT_CENTER.0 | DT_VCENTER.0 | DT_SINGLELINE.0);
    });
}

pub fn build_about_zones(s: f32) -> Vec<HitZone> {
    vec![
        HitZone {
            id: 1,
            rect: srect(20, 328, 244, 44, s),
            action: HitAction::OpenWebsite,
        },
        HitZone {
            id: 2,
            rect: srect(276, 328, 244, 44, s),
            action: HitAction::CloseWindow,
        },
    ]
}

unsafe fn paint_about_window(hwnd: HWND) {
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);
    let w = (rc.right - rc.left).max(1);
    let h = (rc.bottom - rc.top).max(1);
    let s = window_dpi_scale(hwnd);
    let hov = ABOUT_HOVER.load(Ordering::Acquire);

    let mut c = Canvas::new(w, h, COL_CANVAS);
    let rad = 16.0 * s;

    // Hero tile (x=20, y=20, w=500, h=116)
    let hero = srect(20, 20, 500, 116, s);
    c.fill_round_rect(hero, rad, COL_TILE_HERO, 1.0);
    c.fill_radial_glow_clipped(hero, rad, 76.0 * s, 78.0 * s, 120.0 * s, COL_CYAN, 0.18);
    c.stroke_round_rect(hero, rad, 1.0 * s, COL_BORDER_HERO, 1.0);
    c.blit_logo(sc(34, s), sc(36, s), sc(84, s));

    // 4 Bento spec tiles (2x2 asymmetric grid)
    let specs = [
        (20, 148, 244, 78),
        (276, 148, 244, 78),
        (20, 238, 244, 78),
        (276, 238, 244, 78),
    ];
    for &(x, y, tw, th) in &specs {
        let tr = srect(x, y, tw, th, s);
        c.fill_round_rect(tr, 14.0 * s, COL_TILE, 1.0);
        c.stroke_round_rect(tr, 14.0 * s, 1.0 * s, COL_BORDER, 1.0);
    }

    // Bottom buttons
    let b1 = srect(20, 328, 244, 44, s);
    c.fill_round_rect(b1, 12.0 * s, if hov == 1 { COL_SUB_HOVER } else { COL_TILE }, 1.0);
    c.stroke_round_rect(b1, 12.0 * s, 1.0 * s, COL_BORDER, 1.0);

    let b2 = srect(276, 328, 244, 44, s);
    c.fill_round_rect(b2, 12.0 * s, if hov == 2 { COL_CYAN_HOVER } else { COL_CYAN }, 1.0);

    present_canvas_with_text(hwnd, &c, s, |hdc, f| {
        draw_text_str(hdc, f.hero, COL_TEXT, srect(134, 34, 240, 30, s), "SmartBlink", DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0);
        draw_text_str(hdc, f.mono, COL_CYAN, srect(272, 37, 120, 24, s), "v1.0.0 · MIT", DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0);
        draw_text_str(
            hdc,
            f.body,
            COL_TEXT_SUB,
            srect(134, 68, 370, 52, s),
            "Wrong-layout typing detector & one-gesture fix for Windows.\r\nPublisher: VKV Studio (vkvstudio.com)",
            DT_LEFT.0 | DT_WORDBREAK.0,
        );

        let spec_txt = [
            (20, 148, "20 Language Profiles", "Embedded n-gram & script models (<60 KB)"),
            (276, 148, "< 0.1 ms Hook Thread", "Lock-free SPSC ring, zero input lag"),
            (20, 238, "100% RAM-Only Privacy", "Volatile-zeroed buffers, zero telemetry"),
            (276, 238, "Net-Zero Alert Engine", "Exact LED restore + WASAPI knock"),
        ];
        for &(x, y, h_txt, b_txt) in &spec_txt {
            draw_text_str(hdc, f.title, COL_CYAN, srect(x + 16, y + 14, 212, 22, s), h_txt, DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0);
            draw_text_str(hdc, f.caption, COL_TEXT_SUB, srect(x + 16, y + 40, 212, 26, s), b_txt, DT_LEFT.0 | DT_WORDBREAK.0);
        }

        draw_text_str(hdc, f.card_title, COL_TEXT, b1, "vkvstudio.com ↗", DT_CENTER.0 | DT_VCENTER.0 | DT_SINGLELINE.0);
        draw_text_str(hdc, f.card_title, COL_CANVAS, b2, "Close", DT_CENTER.0 | DT_VCENTER.0 | DT_SINGLELINE.0);
    });
}

pub fn build_flyout_zones(s: f32, _cfg: &Config) -> Vec<HitZone> {
    vec![
        // 1. Engine Pause/Resume toggle in Hero Header
        HitZone { id: 1, rect: srect(256, 26, 86, 32, s), action: HitAction::TogglePause },
        // 2..5. Alert Mode 4 pills
        HitZone { id: 2, rect: srect(24, 122, 152, 28, s), action: HitAction::SetNotify(NotifyMode::Both) },
        HitZone { id: 3, rect: srect(192, 122, 152, 28, s), action: HitAction::SetNotify(NotifyMode::Sound) },
        HitZone { id: 4, rect: srect(24, 156, 152, 28, s), action: HitAction::SetNotify(NotifyMode::Led) },
        HitZone { id: 5, rect: srect(192, 156, 152, 28, s), action: HitAction::SetNotify(NotifyMode::Silent) },
        // 6..7. LED Mode 2 pills
        HitZone { id: 6, rect: srect(24, 226, 152, 30, s), action: HitAction::SetLed(LedMode::ScrollLock) },
        HitZone { id: 7, rect: srect(192, 226, 152, 30, s), action: HitAction::SetLed(LedMode::CapsLockGuarded) },
        // 8..10. Fix Gesture 3 pills
        HitZone { id: 8, rect: srect(24, 298, 102, 30, s), action: HitAction::SetFix(FixTrigger::DoubleLeftShift) },
        HitZone { id: 9, rect: srect(133, 298, 102, 30, s), action: HitAction::SetFix(FixTrigger::DoubleLeftCtrl) },
        HitZone { id: 10, rect: srect(242, 298, 102, 30, s), action: HitAction::SetFix(FixTrigger::PauseBreak) },
        // 11..12. Startup & Admin Task tiles
        HitZone { id: 11, rect: srect(14, 344, 164, 52, s), action: HitAction::ToggleAutostart },
        HitZone { id: 12, rect: srect(190, 344, 164, 52, s), action: HitAction::ToggleElevatedTask },
        // 13..15. Bottom Bar: Settings, About, Exit
        HitZone { id: 13, rect: srect(14, 408, 134, 48, s), action: HitAction::OpenSettingsWindow },
        HitZone { id: 14, rect: srect(158, 408, 96, 48, s), action: HitAction::OpenAboutWindow },
        HitZone { id: 15, rect: srect(264, 408, 90, 48, s), action: HitAction::ExitApp },
    ]
}

unsafe fn paint_flyout_window(hwnd: HWND) {
    let mut rc = RECT::default();
    let _ = GetClientRect(hwnd, &mut rc);
    let w = (rc.right - rc.left).max(1);
    let h = (rc.bottom - rc.top).max(1);
    let s = window_dpi_scale(hwnd);
    let cfg = config::get_runtime_config();
    let paused = hooks::is_paused();
    let run_on = autostart::is_registry_autostart_enabled();
    let task_on = autostart::is_elevated_task_registered();
    let hov = FLYOUT_HOVER.load(Ordering::Acquire);

    let mut c = Canvas::new(w, h, COL_CANVAS);
    c.stroke_round_rect(rect(0, 0, w, h), 14.0 * s, 1.2 * s, COL_BORDER_HERO, 1.0);

    // Header Hero Tile
    let hdr = srect(14, 14, 340, 62, s);
    c.fill_round_rect(hdr, 14.0 * s, COL_TILE_HERO, 1.0);
    c.fill_radial_glow_clipped(hdr, 14.0 * s, 46.0 * s, 45.0 * s, 80.0 * s, COL_CYAN, 0.18);
    c.stroke_round_rect(hdr, 14.0 * s, 1.0 * s, COL_BORDER_HERO, 1.0);
    c.blit_logo(sc(22, s), sc(21, s), sc(48, s));
    c.draw_toggle_switch(sc(336, s), sc(45, s), s, !paused);

    // Alert + LED Tile
    let mid1 = srect(14, 86, 340, 180, s);
    c.fill_round_rect(mid1, 14.0 * s, COL_TILE, 1.0);
    c.stroke_round_rect(mid1, 14.0 * s, 1.0 * s, COL_BORDER, 1.0);

    for &(id, x, y, mode) in &[
        (2isize, 24, 122, NotifyMode::Both),
        (3, 192, 122, NotifyMode::Sound),
        (4, 24, 156, NotifyMode::Led),
        (5, 192, 156, NotifyMode::Silent),
    ] {
        let pr = srect(x, y, 152, 28, s);
        let act = cfg.notify_mode == mode;
        c.fill_round_rect(pr, 8.0 * s, if act { COL_SUB_ACTIVE } else if hov == id { COL_SUB_HOVER } else { COL_SUB }, 1.0);
        c.stroke_round_rect(pr, 8.0 * s, 1.0 * s, if act { COL_CYAN } else { COL_BORDER }, 1.0);
    }

    for &(id, x, mode) in &[(6isize, 24, LedMode::ScrollLock), (7, 192, LedMode::CapsLockGuarded)] {
        let pr = srect(x, 226, 152, 30, s);
        let act = cfg.led_mode == mode;
        c.fill_round_rect(pr, 8.0 * s, if act { COL_SUB_ACTIVE } else if hov == id { COL_SUB_HOVER } else { COL_SUB }, 1.0);
        c.stroke_round_rect(pr, 8.0 * s, 1.0 * s, if act { COL_CYAN } else { COL_BORDER }, 1.0);
    }

    // Fix Gesture Tile
    let mid2 = srect(14, 274, 340, 62, s);
    c.fill_round_rect(mid2, 14.0 * s, COL_TILE, 1.0);
    c.stroke_round_rect(mid2, 14.0 * s, 1.0 * s, COL_BORDER, 1.0);
    for &(id, x, trig) in &[
        (8isize, 24, FixTrigger::DoubleLeftShift),
        (9, 133, FixTrigger::DoubleLeftCtrl),
        (10, 242, FixTrigger::PauseBreak),
    ] {
        let pr = srect(x, 298, 102, 30, s);
        let act = cfg.fix_trigger == trig;
        c.fill_round_rect(pr, 8.0 * s, if act { COL_SUB_ACTIVE } else if hov == id { COL_SUB_HOVER } else { COL_SUB }, 1.0);
        c.stroke_round_rect(pr, 8.0 * s, 1.0 * s, if act { COL_CYAN } else { COL_BORDER }, 1.0);
    }

    // Startup & Admin 2 Bento Tiles
    for &(id, x, is_on) in &[(11isize, 14, run_on), (12, 190, task_on)] {
        let tr = srect(x, 344, 164, 52, s);
        c.fill_round_rect(tr, 12.0 * s, if hov == id { COL_SUB_HOVER } else { COL_TILE }, 1.0);
        c.stroke_round_rect(tr, 12.0 * s, 1.0 * s, if is_on { COL_CYAN } else { COL_BORDER }, 0.8);
        c.fill_circle((x + 144) as f32 * s, 370.0 * s, 5.0 * s, if is_on { COL_CYAN } else { COL_TEXT_MUTED }, 1.0);
    }

    // Bottom Action Bar
    let b_set = srect(14, 408, 134, 48, s);
    c.fill_round_rect(b_set, 12.0 * s, if hov == 13 { COL_CYAN_HOVER } else { COL_CYAN }, 1.0);
    let b_abt = srect(158, 408, 96, 48, s);
    c.fill_round_rect(b_abt, 12.0 * s, if hov == 14 { COL_SUB_HOVER } else { COL_TILE }, 1.0);
    c.stroke_round_rect(b_abt, 12.0 * s, 1.0 * s, COL_BORDER, 1.0);
    let b_ext = srect(264, 408, 90, 48, s);
    c.fill_round_rect(b_ext, 12.0 * s, if hov == 15 { COL_RED_BG } else { COL_TILE }, 1.0);
    c.stroke_round_rect(b_ext, 12.0 * s, 1.0 * s, if hov == 15 { COL_RED_BORDER } else { COL_BORDER }, 1.0);

    present_canvas_with_text(hwnd, &c, s, |hdc, f| {
        draw_text_str(hdc, f.title, COL_TEXT, srect(78, 22, 180, 22, s), "SmartBlink", DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0);
        draw_text_str(
            hdc,
            f.caption,
            if paused { COL_AMBER } else { COL_CYAN },
            srect(78, 44, 180, 20, s),
            if paused { "Paused — click switch" } else { "Active & Monitoring" },
            DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0,
        );

        draw_text_str(hdc, f.caption, COL_SLATE, srect(24, 96, 200, 20, s), "ALERT MODE", DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0);
        for &(x, y, txt) in &[
            (24, 122, "Both (Click+LED)"),
            (192, 122, "Audio Only"),
            (24, 156, "LED Only"),
            (192, 156, "Silent"),
        ] {
            draw_text_str(hdc, f.caption, COL_TEXT, srect(x, y, 152, 28, s), txt, DT_CENTER.0 | DT_VCENTER.0 | DT_SINGLELINE.0);
        }

        draw_text_str(hdc, f.caption, COL_SLATE, srect(24, 198, 200, 20, s), "LED TARGET", DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0);
        draw_text_str(hdc, f.caption, COL_TEXT, srect(24, 226, 152, 30, s), "Scroll Lock", DT_CENTER.0 | DT_VCENTER.0 | DT_SINGLELINE.0);
        draw_text_str(hdc, f.caption, COL_TEXT, srect(192, 226, 152, 30, s), "Caps Lock", DT_CENTER.0 | DT_VCENTER.0 | DT_SINGLELINE.0);

        draw_text_str(hdc, f.caption, COL_SLATE, srect(24, 276, 200, 20, s), "FIX HOTKEY", DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0);
        draw_text_str(hdc, f.mono, COL_TEXT, srect(24, 298, 102, 30, s), "2x LShift", DT_CENTER.0 | DT_VCENTER.0 | DT_SINGLELINE.0);
        draw_text_str(hdc, f.mono, COL_TEXT, srect(133, 298, 102, 30, s), "2x LCtrl", DT_CENTER.0 | DT_VCENTER.0 | DT_SINGLELINE.0);
        draw_text_str(hdc, f.mono, COL_TEXT, srect(242, 298, 102, 30, s), "Pause", DT_CENTER.0 | DT_VCENTER.0 | DT_SINGLELINE.0);

        draw_text_str(hdc, f.card_title, COL_TEXT, srect(26, 350, 120, 20, s), "Startup", DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0);
        draw_text_str(hdc, f.caption, COL_TEXT_SUB, srect(26, 370, 120, 18, s), if run_on { "Enabled" } else { "Disabled" }, DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0);

        draw_text_str(hdc, f.card_title, COL_TEXT, srect(202, 350, 120, 20, s), "Admin Task", DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0);
        draw_text_str(hdc, f.caption, COL_TEXT_SUB, srect(202, 370, 120, 18, s), if task_on { "Registered" } else { "Standard" }, DT_LEFT.0 | DT_VCENTER.0 | DT_SINGLELINE.0);

        draw_text_str(hdc, f.card_title, COL_CANVAS, b_set, "Settings...", DT_CENTER.0 | DT_VCENTER.0 | DT_SINGLELINE.0);
        draw_text_str(hdc, f.card_title, COL_TEXT, b_abt, "About", DT_CENTER.0 | DT_VCENTER.0 | DT_SINGLELINE.0);
        draw_text_str(hdc, f.card_title, COL_TEXT, b_ext, "Exit", DT_CENTER.0 | DT_VCENTER.0 | DT_SINGLELINE.0);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_cycle_helpers_cover_all_variants() {
        assert_eq!(next_notify_mode(NotifyMode::Both), NotifyMode::Sound);
        assert_eq!(next_notify_mode(NotifyMode::Sound), NotifyMode::Led);
        assert_eq!(next_notify_mode(NotifyMode::Led), NotifyMode::Silent);
        assert_eq!(next_notify_mode(NotifyMode::Silent), NotifyMode::Both);

        assert_eq!(next_led_mode(LedMode::ScrollLock), LedMode::CapsLockGuarded);
        assert_eq!(next_led_mode(LedMode::CapsLockGuarded), LedMode::ScrollLock);

        assert_eq!(next_fix_trigger(FixTrigger::DoubleLeftShift), FixTrigger::DoubleLeftCtrl);
        assert_eq!(next_fix_trigger(FixTrigger::DoubleLeftCtrl), FixTrigger::PauseBreak);
        assert_eq!(next_fix_trigger(FixTrigger::PauseBreak), FixTrigger::DoubleLeftShift);
    }

    #[test]
    fn logo_vector_rasterizer_renders_cyan_and_slate_strokes() {
        let rgba = render_logo_rgba(64);
        assert_eq!(rgba.len(), 64 * 64 * 4);
        let mut has_cyan = false;
        let mut has_slate = false;
        for px in rgba.chunks_exact(4) {
            if px[3] > 180 && px[0] < 30 && px[1] > 200 && px[2] > 230 {
                has_cyan = true;
            }
            if px[3] > 60 && px[0] > 100 && px[1] > 110 && px[2] > 120 && (px[2] as i32 - px[0] as i32) < 45 {
                has_slate = true;
            }
        }
        assert!(has_cyan, "rendered smartblink-logo.svg must contain #00e5ff cyan pixels");
        assert!(has_slate, "rendered smartblink-logo.svg must contain #8b95a5 slate pixels");
    }

    #[test]
    fn bento_settings_zones_are_non_overlapping() {
        let cfg = Config::default();
        let zones = build_settings_zones(1.0, &cfg);
        assert!(zones.len() >= 15);
        for i in 0..zones.len() {
            for j in (i + 1)..zones.len() {
                let a = zones[i].rect;
                let b = zones[j].rect;
                let overlap = a.left < b.right && a.right > b.left && a.top < b.bottom && a.bottom > b.top;
                assert!(!overlap, "Bento hit zones {} and {} must not overlap", zones[i].id, zones[j].id);
            }
        }
    }
}

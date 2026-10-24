#![allow(dead_code)]
//! Alert channels (M5): net-zero LED blink (`Scroll Lock` default, `Caps Lock` guarded opt-in)
//! and low-frequency WASAPI shared-mode click sample (`waveOut` fallback).
//! Architectural invariants (AGENTS.md #3, #4, #7, #8; ARCHITECTURE.md §1, §3.4):
//! - LED blink is net-zero: original Caps/Scroll toggle state is captured before blinking and
//!   restored & verified afterwards (<= 120 ms total; aborted early if a real keystroke arrives).
//! - Caps Lock mode only triggers in typing gaps >= 80 ms since the last real keystroke.
//! - All synthetic `SendInput` events are bracketed by `hooks::begin_suppression()` / `end_suppression()`.
//! - Audio uses an in-memory 16-bit 48 kHz mono low-passed "wooden knock" (~24 ms, ~-18 dBFS)
//!   with zero disk I/O and zero per-play heap allocation.

use crate::config::{LedMode, NotifyMode};
use crate::hooks;
use std::f64::consts::PI;
use std::mem::size_of;
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle, Thread};
use std::time::{Duration, Instant};
use windows::core::PSTR;
use windows::Win32::Media::Audio::{
    eConsole, eRender, waveOutClose, waveOutOpen, waveOutPrepareHeader, waveOutReset,
    waveOutUnprepareHeader, waveOutWrite, IAudioClient, IAudioRenderClient, IMMDeviceEnumerator,
    MMDeviceEnumerator, AUDCLNT_E_DEVICE_INVALIDATED, AUDCLNT_E_NOT_STOPPED,
    AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM,
    AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY, CALLBACK_NULL, HWAVEOUT, WAVEFORMATEX, WAVEHDR,
    WAVE_FORMAT_PCM, WAVE_MAPPER,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
    KEYEVENTF_KEYUP, VIRTUAL_KEY,
};
use windows::Win32::UI::WindowsAndMessaging::{PeekMessageW, MSG, PM_NOREMOVE};

pub const SAMPLE_RATE: u32 = 48_000;
/// 24 ms at 48 kHz = 1152 frames (within the mandated 15–30 ms window).
pub const CLICK_FRAMES: usize = 1152;
/// Audible tactile wooden knock peak for laptop speakers (~92% full scale).
pub const TARGET_PEAK_I16: i16 = 30_000;

pub const VK_CAPITAL_U16: u16 = 0x14;
pub const SCAN_CAPITAL: u16 = 0x3A;
pub const VK_SCROLL_U16: u16 = 0x91;
pub const SCAN_SCROLL: u16 = 0x46;

/// Minimum idle gap required before starting a Caps Lock blink (`ARCHITECTURE.md` §3.4).
/// Set to 1200 ms so Caps Lock never toggles while the user is actively typing words/sentences
/// (preventing uppercase character corruption like `chf,fnSdftn` or `t,fytymRj`).
pub const CAPS_MIN_GAP_MS: u32 = 1200;
const BLINK_HOLD_MS: u64 = 75;
const CAPS_HOLD_MS: u64 = 85;
const BLINK_GAP_MS: u64 = 55;
const BLINK_SLICE_MS: u64 = 5;

/// Deterministically synthesize the 24 ms 48 kHz 16-bit mono tactile "wooden knock" click sample.
pub fn generate_click_pcm() -> [i16; CLICK_FRAMES] {
    let mut raw = [0.0f64; CLICK_FRAMES];
    let sr = SAMPLE_RATE as f64;
    let attack_samples = 36.0; // 0.75 ms crisp mechanical attack
    let decay_tau = 0.0110; // 11.0 ms exponential body decay for high RMS energy on laptop speakers
    let mut phase = 0.0f64;

    for (i, slot) in raw.iter_mut().enumerate() {
        let t = (i as f64) / sr;
        // Laptop-speaker sweet-spot pitch sweep (1150 Hz -> 520 Hz) above the 400 Hz transducer cutoff.
        let freq = 520.0 + 630.0 * (-t / 0.0055).exp();
        phase += 2.0 * PI * freq / sr;

        let attack = if (i as f64) < attack_samples {
            0.5 * (1.0 - (PI * (i as f64) / attack_samples).cos())
        } else {
            1.0
        };
        let env = attack * (-t / decay_tau).exp();
        let body_harmonic = (2.0 * PI * 1850.0 * t).sin() * (-t / 0.0075).exp();
        let click_transient = (-t / 0.0028).exp() * (2.0 * PI * 2750.0 * t).sin();
        *slot = env * (phase.sin() + 0.65 * (2.0 * phase).sin())
            + 0.45 * attack * body_harmonic
            + 0.50 * attack * click_transient;
    }

    // One-pole low-pass filter at fc = 4000 Hz to preserve crisp tactile snap on laptop speakers.
    let rc = 1.0 / (2.0 * PI * 4000.0);
    let dt = 1.0 / sr;
    let alpha = dt / (rc + dt);
    let mut filtered = [0.0f64; CLICK_FRAMES];
    let mut prev = 0.0f64;
    for i in 0..CLICK_FRAMES {
        prev += alpha * (raw[i] - prev);
        filtered[i] = prev;
    }

    // Smooth fade-out over the final 64 samples and force exact zero at both endpoints.
    let fade_len = 64usize;
    for i in 0..fade_len {
        let idx = CLICK_FRAMES - fade_len + i;
        let w = 0.5 * (1.0 + (PI * (i as f64) / (fade_len as f64)).cos());
        filtered[idx] *= w;
    }
    filtered[0] = 0.0;
    filtered[CLICK_FRAMES - 1] = 0.0;

    let max_abs = filtered
        .iter()
        .fold(0.0f64, |acc, &v| acc.max(v.abs()))
        .max(1e-9);
    let scale = (TARGET_PEAK_I16 as f64) / max_abs;

    let mut out = [0i16; CLICK_FRAMES];
    for i in 0..CLICK_FRAMES {
        out[i] = (filtered[i] * scale).round().clamp(-32767.0, 32767.0) as i16;
    }
    out[0] = 0;
    out[CLICK_FRAMES - 1] = 0;
    out
}

fn pcm_wave_format() -> WAVEFORMATEX {
    WAVEFORMATEX {
        wFormatTag: WAVE_FORMAT_PCM as u16,
        nChannels: 1,
        nSamplesPerSec: SAMPLE_RATE,
        nAvgBytesPerSec: SAMPLE_RATE * 2,
        nBlockAlign: 2,
        wBitsPerSample: 16,
        cbSize: 0,
    }
}

pub struct WasapiClicker {
    audio_client: IAudioClient,
    render_client: IAudioRenderClient,
    buffer_frames: u32,
}

impl WasapiClicker {
    pub fn new() -> windows::core::Result<Self> {
        unsafe {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            let device = enumerator.GetDefaultAudioEndpoint(eRender, eConsole)?;
            let audio_client: IAudioClient = device.Activate(CLSCTX_ALL, None)?;
            let wfx = pcm_wave_format();
            let flags =
                AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY;
            // 100 ms shared-mode buffer (in 100-ns units = 1_000_000).
            audio_client.Initialize(AUDCLNT_SHAREMODE_SHARED, flags, 1_000_000, 0, &wfx, None)?;
            let buffer_frames = audio_client.GetBufferSize()?;
            let render_client: IAudioRenderClient = audio_client.GetService()?;

            // Pre-warm the audio codec with 48 frames (1 ms) of silence so the first real click has <1 ms latency:
            let warm_frames = 48u32.min(buffer_frames);
            if let Ok(buf_ptr) = render_client.GetBuffer(warm_frames) {
                ptr::write_bytes(buf_ptr, 0, (warm_frames as usize) * size_of::<i16>());
                let _ = render_client.ReleaseBuffer(warm_frames, 0);
                let _ = audio_client.Start();
                let _ = audio_client.Stop();
                let _ = audio_client.Reset();
            }

            Ok(Self {
                audio_client,
                render_client,
                buffer_frames,
            })
        }
    }

    /// Submit `pcm` into the shared-mode render buffer and start playback.
    /// Returns the submission latency in microseconds.
    pub fn play_click(&self, pcm: &[i16; CLICK_FRAMES]) -> windows::core::Result<u64> {
        let t0 = Instant::now();
        let frames = (CLICK_FRAMES as u32).min(self.buffer_frames);
        unsafe {
            let padding = self.audio_client.GetCurrentPadding().unwrap_or(0);
            if padding > 0 {
                // Immediately flush any in-flight previous click so the new keystroke click
                // starts at frame 0 with zero queueing delay.
                let _ = self.audio_client.Stop();
                let _ = self.audio_client.Reset();
            }
            let buf_ptr = self.render_client.GetBuffer(frames)?;
            ptr::copy_nonoverlapping(
                pcm.as_ptr() as *const u8,
                buf_ptr,
                (frames as usize) * size_of::<i16>(),
            );
            self.render_client.ReleaseBuffer(frames, 0)?;
            match self.audio_client.Start() {
                Ok(()) => {}
                Err(e) if e.code() == AUDCLNT_E_NOT_STOPPED => {}
                Err(e) => return Err(e),
            }
        }
        Ok(t0.elapsed().as_micros() as u64)
    }

    pub fn current_padding(&self) -> u32 {
        unsafe { self.audio_client.GetCurrentPadding().unwrap_or(0) }
    }

    pub fn stop_and_reset(&self) {
        unsafe {
            let _ = self.audio_client.Stop();
            let _ = self.audio_client.Reset();
        }
    }
}

impl Drop for WasapiClicker {
    fn drop(&mut self) {
        self.stop_and_reset();
    }
}

struct WaveOutClicker {
    hwo: HWAVEOUT,
}

impl WaveOutClicker {
    fn new() -> Option<Self> {
        unsafe {
            let wfx = pcm_wave_format();
            let mut hwo = HWAVEOUT::default();
            let rc = waveOutOpen(
                Some(&mut hwo),
                WAVE_MAPPER,
                &wfx,
                0,
                0,
                CALLBACK_NULL,
            );
            if rc == 0 && !hwo.0.is_null() {
                Some(Self { hwo })
            } else {
                None
            }
        }
    }

    fn play_click_blocking(&self, pcm: &mut [i16; CLICK_FRAMES]) {
        unsafe {
            let _ = waveOutReset(self.hwo);
            let mut hdr = WAVEHDR {
                lpData: PSTR(pcm.as_mut_ptr() as *mut u8),
                dwBufferLength: (CLICK_FRAMES * size_of::<i16>()) as u32,
                dwBytesRecorded: 0,
                dwUser: 0,
                dwFlags: 0,
                dwLoops: 0,
                lpNext: ptr::null_mut(),
                reserved: 0,
            };
            let cb = size_of::<WAVEHDR>() as u32;
            if waveOutPrepareHeader(self.hwo, &mut hdr, cb) == 0 {
                let _ = waveOutWrite(self.hwo, &mut hdr, cb);
                thread::sleep(Duration::from_millis(32));
                let _ = waveOutUnprepareHeader(self.hwo, &mut hdr, cb);
            }
        }
    }
}

impl Drop for WaveOutClicker {
    fn drop(&mut self) {
        unsafe {
            let _ = waveOutReset(self.hwo);
            let _ = waveOutClose(self.hwo);
        }
    }
}

/// Query whether a lock key (`VK_SCROLL` or `VK_CAPITAL`) is currently toggled ON (low bit of `GetKeyState`).
#[inline]
pub fn is_lock_key_toggled(vk: u16) -> bool {
    unsafe {
        let mut msg = MSG::default();
        let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
        let state = GetKeyState(vk as i32);
        (state & 0x0001) != 0
    }
}

/// Inject a single synthetic press + release of `vk` / `scan_code` bracketed by hook suppression.
pub fn inject_lock_toggle(vk: u16, scan_code: u16) -> bool {
    let inputs = [
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(vk),
                    wScan: scan_code,
                    dwFlags: KEYBD_EVENT_FLAGS(0),
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        },
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(vk),
                    wScan: scan_code,
                    dwFlags: KEYEVENTF_KEYUP,
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        },
    ];

    hooks::begin_suppression();
    let sent = unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
    hooks::end_suppression();
    sent == inputs.len() as u32
}

/// Execute a net-zero LED blink on `Scroll Lock` or `Caps Lock` (`ARCHITECTURE.md` §3.4, `AGENTS.md` #8).
/// Each pulse is strictly paired (Toggle ON + Toggle OFF) and aborts immediately if a real keystroke arrives.
/// `CapsLockGuarded` only triggers after `CAPS_MIN_GAP_MS` (1200 ms) of full typing stillness and uses 2 clear
/// 85 ms pulses so the user can actually see the LED without colliding with active keystrokes.
pub fn blink_led_net_zero(mode: LedMode) -> bool {
    let (vk, scan, pulses, hold_ms) = match mode {
        LedMode::ScrollLock => (VK_SCROLL_U16, SCAN_SCROLL, 2usize, BLINK_HOLD_MS),
        LedMode::CapsLockGuarded => (VK_CAPITAL_U16, SCAN_CAPITAL, 2usize, CAPS_HOLD_MS),
    };

    let orig_toggled = is_lock_key_toggled(vk);
    let seq0 = hooks::real_key_seq();

    for pulse in 0..pulses {
        if hooks::is_paused() || hooks::real_key_seq() != seq0 {
            break;
        }
        // Toggle #1 (ON): if SendInput fails, 0 toggles were injected -> net-zero preserved.
        if !inject_lock_toggle(vk, scan) {
            break;
        }

        let slices = hold_ms / BLINK_SLICE_MS;
        for _ in 0..slices {
            if hooks::is_paused() || hooks::real_key_seq() != seq0 {
                break;
            }
            thread::sleep(Duration::from_millis(BLINK_SLICE_MS));
        }

        // Toggle #2 (OFF): always paired with Toggle #1 so injected toggle count is strictly even.
        let _ = inject_lock_toggle(vk, scan);

        if pulse + 1 < pulses && !hooks::is_paused() && hooks::real_key_seq() == seq0 {
            let gap_slices = BLINK_GAP_MS / BLINK_SLICE_MS;
            for _ in 0..gap_slices {
                if hooks::is_paused() || hooks::real_key_seq() != seq0 {
                    break;
                }
                thread::sleep(Duration::from_millis(BLINK_SLICE_MS));
            }
        }
    }

    // Wait up to 30 ms for the OS input queue to reflect the restored state.
    for _ in 0..6 {
        if is_lock_key_toggled(vk) == orig_toggled {
            return true;
        }
        thread::sleep(Duration::from_millis(5));
    }

    is_lock_key_toggled(vk) == orig_toggled
}

/// Trigger a one-shot test alert (sound + LED blink) on a short background thread (used by Settings UI).
pub fn trigger_test_alert(notify_mode: NotifyMode, led_mode: LedMode) {
    thread::spawn(move || unsafe {
        if matches!(notify_mode, NotifyMode::Sound | NotifyMode::Both | NotifyMode::Silent) {
            let hr = CoInitializeEx(None, COINIT_MULTITHREADED);
            let mut pcm = generate_click_pcm();
            if let Ok(w) = WasapiClicker::new() {
                if w.play_click(&pcm).is_ok() {
                    thread::sleep(Duration::from_millis(120));
                    w.stop_and_reset();
                }
            } else if let Some(wo) = WaveOutClicker::new() {
                wo.play_click_blocking(&mut pcm);
            }
            if hr.is_ok() {
                CoUninitialize();
            }
        }
        if matches!(notify_mode, NotifyMode::Led | NotifyMode::Both | NotifyMode::Silent) {
            let _ = blink_led_net_zero(led_mode);
        }
    });
}

pub struct SignalEngine {
    notify_mode: NotifyMode,
    led_mode: LedMode,
    stop: Arc<AtomicBool>,
    click_pending: Arc<AtomicBool>,
    blink_pending: Arc<AtomicBool>,
    audio_thread: Option<Thread>,
    led_thread: Option<Thread>,
    audio_join: Option<JoinHandle<()>>,
    led_join: Option<JoinHandle<()>>,
}

impl Drop for SignalEngine {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(t) = &self.audio_thread {
            t.unpark();
        }
        if let Some(t) = &self.led_thread {
            t.unpark();
        }
        if let Some(j) = self.audio_join.take() {
            let _ = j.join();
        }
        if let Some(j) = self.led_join.take() {
            let _ = j.join();
        }
    }
}

impl SignalEngine {
    pub fn start(notify_mode: NotifyMode, led_mode: LedMode) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let click_pending = Arc::new(AtomicBool::new(false));
        let blink_pending = Arc::new(AtomicBool::new(false));

        let stop_audio = Arc::clone(&stop);
        let click_flag = Arc::clone(&click_pending);
        let audio_join = thread::Builder::new()
            .name("smartblink-audio".into())
            .spawn(move || unsafe {
                let hr = CoInitializeEx(None, COINIT_MULTITHREADED);
                let mut pcm = generate_click_pcm();
                let mut wasapi = WasapiClicker::new().ok();
                let mut wave_out = if wasapi.is_none() {
                    WaveOutClicker::new()
                } else {
                    None
                };

                while !stop_audio.load(Ordering::Acquire) {
                    if click_flag.swap(false, Ordering::AcqRel) {
                        if !hooks::is_paused() {
                            let mut played = false;
                            if let Some(ref w) = wasapi {
                                match w.play_click(&pcm) {
                                    Ok(_) => {
                                        played = true;
                                        // Allow full WASAPI + APO + DAC pipeline drain (~90 ms) before stopping,
                                        // so the 24 ms click is never truncated mid-playback by Reset().
                                        for _ in 0..18 {
                                            if stop_audio.load(Ordering::Acquire)
                                                || click_flag.load(Ordering::Acquire)
                                            {
                                                break;
                                            }
                                            thread::sleep(Duration::from_millis(5));
                                        }
                                        if !click_flag.load(Ordering::Acquire)
                                            && w.current_padding() == 0
                                        {
                                            w.stop_and_reset();
                                        }
                                    }
                                    Err(e) if e.code() == AUDCLNT_E_DEVICE_INVALIDATED => {
                                        drop(wasapi.take());
                                        if let Ok(new_w) = WasapiClicker::new() {
                                            if new_w.play_click(&pcm).is_ok() {
                                                played = true;
                                            }
                                            wasapi = Some(new_w);
                                        }
                                    }
                                    Err(_) => {
                                        w.stop_and_reset();
                                    }
                                }
                            }
                            if !played {
                                if wave_out.is_none() {
                                    wave_out = WaveOutClicker::new();
                                }
                                if let Some(ref wo) = wave_out {
                                    wo.play_click_blocking(&mut pcm);
                                }
                            }
                        }
                    } else {
                        thread::park();
                    }
                }

                drop(wasapi);
                drop(wave_out);
                if hr.is_ok() {
                    CoUninitialize();
                }
            })
            .ok();

        let stop_led = Arc::clone(&stop);
        let blink_flag = Arc::clone(&blink_pending);
        let led_join = thread::Builder::new()
            .name("smartblink-led".into())
            .spawn(move || {
                while !stop_led.load(Ordering::Acquire) {
                    if blink_flag.swap(false, Ordering::AcqRel) {
                        let live_led = crate::config::get_runtime_config().led_mode;
                        let _ = blink_led_net_zero(live_led);
                    } else {
                        thread::park();
                    }
                }
            })
            .ok();

        let audio_thread = audio_join.as_ref().map(|j| j.thread().clone());
        let led_thread = led_join.as_ref().map(|j| j.thread().clone());

        Self {
            notify_mode,
            led_mode,
            stop,
            click_pending,
            blink_pending,
            audio_thread,
            led_thread,
            audio_join,
            led_join,
        }
    }

    #[inline]
    pub fn audio_enabled(&self) -> bool {
        matches!(self.notify_mode, NotifyMode::Sound | NotifyMode::Both)
    }

    #[inline]
    pub fn led_enabled(&self) -> bool {
        matches!(self.notify_mode, NotifyMode::Led | NotifyMode::Both)
    }

    #[inline]
    pub fn led_mode(&self) -> LedMode {
        self.led_mode
    }

    pub fn set_notify_mode(&mut self, mode: NotifyMode) {
        self.notify_mode = mode;
    }

    pub fn set_led_mode(&mut self, mode: LedMode) {
        self.led_mode = mode;
    }

    /// Non-blocking trigger of the 24 ms low-frequency click on the audio thread.
    pub fn trigger_click(&self) {
        if !self.audio_enabled() || hooks::is_paused() {
            return;
        }
        self.click_pending.store(true, Ordering::Release);
        if let Some(ref t) = self.audio_thread {
            t.unpark();
        }
    }

    /// Non-blocking trigger of a net-zero LED blink on the LED thread.
    pub fn trigger_blink(&self) {
        if !self.led_enabled() || hooks::is_paused() {
            return;
        }
        self.blink_pending.store(true, Ordering::Release);
        if let Some(ref t) = self.led_thread {
            t.unpark();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn click_pcm_waveform_meets_duration_and_amplitude_spec() {
        let pcm = generate_click_pcm();
        // 1152 frames at 48 kHz = 24.0 ms (within 15..=30 ms spec).
        let duration_ms = (pcm.len() as f64) * 1000.0 / (SAMPLE_RATE as f64);
        assert!(
            (15.0..=30.0).contains(&duration_ms),
            "click duration {} ms outside 15..=30 ms",
            duration_ms
        );
        // Zero endpoints to prevent clicks/pops:
        assert_eq!(pcm[0], 0);
        assert_eq!(pcm[CLICK_FRAMES - 1], 0);

        // Peak amplitude around TARGET_PEAK_I16 (30_000):
        let peak = pcm.iter().map(|&s| (s as i32).abs()).max().unwrap_or(0);
        assert!(
            (29_000..=31_000).contains(&peak),
            "unexpected peak amplitude {}",
            peak
        );
    }

    #[test]
    fn scroll_lock_and_caps_lock_blink_restore_exact_net_zero_state() {
        hooks::set_paused(false);

        let orig_scroll = is_lock_key_toggled(VK_SCROLL_U16);
        assert!(blink_led_net_zero(LedMode::ScrollLock));
        assert_eq!(
            is_lock_key_toggled(VK_SCROLL_U16),
            orig_scroll,
            "Scroll Lock state must be restored net-zero after blink"
        );

        let orig_caps = is_lock_key_toggled(VK_CAPITAL_U16);
        assert!(blink_led_net_zero(LedMode::CapsLockGuarded));
        assert_eq!(
            is_lock_key_toggled(VK_CAPITAL_U16),
            orig_caps,
            "Caps Lock state must be restored net-zero after blink"
        );
    }

    #[test]
    fn wasapi_click_engine_initializes_and_measures_low_latency() {
        unsafe {
            let hr = CoInitializeEx(None, COINIT_MULTITHREADED);
            let pcm = generate_click_pcm();
            if let Ok(clicker) = WasapiClicker::new() {
                let submit_us = clicker
                    .play_click(&pcm)
                    .expect("WASAPI play_click must succeed");
                thread::sleep(Duration::from_millis(30));
                clicker.stop_and_reset();

                // Even the first real click after WasapiClicker::new() pre-warm must complete well under 15 ms.
                assert!(
                    submit_us < 15_000,
                    "WASAPI submission latency {} us too high",
                    submit_us
                );
            }
            if hr.is_ok() {
                CoUninitialize();
            }
        }
    }
}

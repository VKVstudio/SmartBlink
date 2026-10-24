//! Lock-free single-producer / single-consumer (SPSC) ring buffer for hook events.
//! Architectural invariants (AGENTS.md #2, #3):
//! - Fixed-size RAM-only buffer; never touches disk or network.
//! - Producer (hook thread) never blocks and never allocates.
//! - Consumer zeroes each slot upon read and zeroes the entire buffer on reset.

use std::cell::UnsafeCell;
use std::ptr;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

pub const RING_CAPACITY: usize = 256; // Must be a power of two.
const RING_MASK: usize = RING_CAPACITY - 1;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputKind {
    Empty = 0,
    KeyDown = 1,
    KeyUp = 2,
    MouseClick = 3,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputRecord {
    pub vk: u16,
    pub scan_code: u16,
    pub flags: u32,
    pub ms_tick: u32,
    pub kind: InputKind,
    pub _pad: [u8; 3],
}

impl Default for InputRecord {
    fn default() -> Self {
        Self::zeroed()
    }
}

impl InputRecord {
    pub const fn zeroed() -> Self {
        Self {
            vk: 0,
            scan_code: 0,
            flags: 0,
            ms_tick: 0,
            kind: InputKind::Empty,
            _pad: [0; 3],
        }
    }
}

pub struct SpscRing {
    head: AtomicUsize,
    tail: AtomicUsize,
    dropped: AtomicU64,
    slots: [UnsafeCell<InputRecord>; RING_CAPACITY],
}

// SAFETY: Single producer (hook thread) writes slots indexed by `head`;
// single consumer (worker thread) reads/zeroes slots indexed by `tail`.
unsafe impl Sync for SpscRing {}

impl SpscRing {
    pub const fn new() -> Self {
        const EMPTY_CELL: UnsafeCell<InputRecord> = UnsafeCell::new(InputRecord::zeroed());
        Self {
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            dropped: AtomicU64::new(0),
            slots: [EMPTY_CELL; RING_CAPACITY],
        }
    }

    /// Producer-only: push one event in O(1) without blocking or allocating.
    /// Returns `false` if the ring is full (event dropped).
    #[inline]
    pub fn push(&self, rec: InputRecord) -> bool {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);
        if head.wrapping_sub(tail) >= RING_CAPACITY {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            return false;
        }
        let idx = head & RING_MASK;
        // SAFETY: Producer owns `slots[idx]` while `head - tail < RING_CAPACITY`.
        unsafe {
            ptr::write(self.slots[idx].get(), rec);
        }
        self.head.store(head.wrapping_add(1), Ordering::Release);
        true
    }

    /// Consumer-only: pop the oldest event and volatile-zero its slot in RAM.
    #[inline]
    pub fn pop(&self) -> Option<InputRecord> {
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.head.load(Ordering::Acquire);
        if tail == head {
            return None;
        }
        let idx = tail & RING_MASK;
        // SAFETY: Consumer owns `slots[idx]` while `tail != head`.
        let rec = unsafe {
            let slot = self.slots[idx].get();
            let val = ptr::read(slot);
            ptr::write_volatile(slot, InputRecord::zeroed());
            val
        };
        self.tail.store(tail.wrapping_add(1), Ordering::Release);
        Some(rec)
    }

    /// Consumer-only: drain all pending events and volatile-zero every slot in the ring.
    pub fn clear_and_zero(&self) {
        while self.pop().is_some() {}
    }

    #[cfg(test)]
    pub fn dropped_count(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
}

static GLOBAL_RING: SpscRing = SpscRing::new();

#[inline]
pub fn global_ring() -> &'static SpscRing {
    &GLOBAL_RING
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_pop_fifo_and_zeroes_slot() {
        let ring = SpscRing::new();
        let rec1 = InputRecord {
            vk: 0x41,
            scan_code: 0x1E,
            flags: 0,
            ms_tick: 100,
            kind: InputKind::KeyDown,
            _pad: [0; 3],
        };
        let rec2 = InputRecord {
            vk: 0x42,
            scan_code: 0x30,
            flags: 0,
            ms_tick: 120,
            kind: InputKind::KeyDown,
            _pad: [0; 3],
        };
        assert!(ring.push(rec1));
        assert!(ring.push(rec2));

        assert_eq!(ring.pop(), Some(rec1));
        // Slot 0 must now be zeroed in RAM.
        let slot0 = unsafe { ptr::read_volatile(ring.slots[0].get()) };
        assert_eq!(slot0, InputRecord::zeroed());

        assert_eq!(ring.pop(), Some(rec2));
        assert_eq!(ring.pop(), None);
    }

    #[test]
    fn overflow_drops_without_blocking() {
        let ring = SpscRing::new();
        for i in 0..RING_CAPACITY {
            let rec = InputRecord {
                vk: i as u16,
                scan_code: 1,
                flags: 0,
                ms_tick: i as u32,
                kind: InputKind::KeyDown,
                _pad: [0; 3],
            };
            assert!(ring.push(rec));
        }
        // Next push must return false and increment dropped counter.
        let extra = InputRecord {
            vk: 999,
            scan_code: 1,
            flags: 0,
            ms_tick: 999,
            kind: InputKind::KeyDown,
            _pad: [0; 3],
        };
        assert!(!ring.push(extra));
        assert_eq!(ring.dropped_count(), 1);

        ring.clear_and_zero();
        assert_eq!(ring.pop(), None);
    }
}

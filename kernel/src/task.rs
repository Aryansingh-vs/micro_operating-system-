use core::sync::atomic::{AtomicU64, Ordering};
use pc_keyboard::{layouts, DecodedKey, HandleControl, KeyState, PS2Keyboard, ScancodeSet1};
use spin::Mutex;

pub static TICK_COUNT: AtomicU64 = AtomicU64::new(0);
pub static KEY_QUEUE: Mutex<[u8; 256]> = Mutex::new([0; 256]);
static KEY_HEAD: AtomicU64 = AtomicU64::new(0);
static KEY_TAIL: AtomicU64 = AtomicU64::new(0);

pub fn push_scancode(scancode: u8) {
    let tail = KEY_TAIL.load(Ordering::Relaxed) as usize;
    let next = (tail + 1) % 256;
    let head = KEY_HEAD.load(Ordering::Relaxed) as usize;
    if next != head {
        KEY_QUEUE.lock()[tail] = scancode;
        KEY_TAIL.store(next as u64, Ordering::Relaxed);
    }
}

pub fn pop_scancode() -> Option<u8> {
    let head = KEY_HEAD.load(Ordering::Relaxed) as usize;
    let tail = KEY_TAIL.load(Ordering::Relaxed) as usize;
    if head == tail {
        return None;
    }
    let value = KEY_QUEUE.lock()[head];
    KEY_HEAD.store(((head + 1) % 256) as u64, Ordering::Relaxed);
    Some(value)
}

pub fn uptime_ticks() -> u64 {
    TICK_COUNT.load(Ordering::Relaxed)
}

pub fn uptime_seconds() -> u64 {
    uptime_ticks() / 18
}

/// Cooperative scheduler marker. Clock, keyboard, and shell jobs yield in this main-loop turn.
pub fn yield_now() {
    core::hint::spin_loop();
}

pub fn keyboard() -> PS2Keyboard<layouts::Us104Key, ScancodeSet1> {
    PS2Keyboard::new(
        ScancodeSet1::new(),
        layouts::Us104Key,
        HandleControl::MapLettersToUnicode,
    )
}

pub fn decoded_key(
    keyboard: &mut PS2Keyboard<layouts::Us104Key, ScancodeSet1>,
    scancode: u8,
) -> Option<DecodedKey> {
    if let Ok(Some(event)) = keyboard.add_byte(scancode) {
        if event.state == KeyState::Down {
            return keyboard.process_keyevent(event);
        }
    }
    None
}

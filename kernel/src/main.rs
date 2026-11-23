#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]

extern crate alloc;

mod apps;
mod fs;
mod gdt;
mod interrupts;
mod keyboard;
mod memory;
mod rtc;
mod serial;
mod shell;
mod task;
mod ui;
mod vga;

use bootloader_api::{
    config::{BootloaderConfig, Mapping},
    entry_point, BootInfo,
};
use pc_keyboard::DecodedKey;
use x86_64::instructions::hlt;
use alloc::string::ToString;

pub static BOOTLOADER_CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.mappings.physical_memory = Some(Mapping::Dynamic);
    #[allow(deprecated)]
    {
        config.frame_buffer.minimum_framebuffer_width = Some(640);
        config.frame_buffer.minimum_framebuffer_height = Some(400);
    }
    config
};

entry_point!(kernel_main, config = &BOOTLOADER_CONFIG);

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    serial::init();
    serial_println!("AryOS boot: serial COM1 initialized");

    gdt::init();
    interrupts::init_idt();
    interrupts::enable();
    serial_println!("AryOS boot: GDT, IDT, PIC initialized");

    let physical_offset = boot_info
        .physical_memory_offset
        .as_ref()
        .copied()
        .expect("physical memory mapping missing");
    let mut mapper = unsafe { memory::init_offset_page_table(physical_offset) };
    let mut frame_allocator =
        memory::BootInfoFrameAllocator::init(&boot_info.memory_regions);
    memory::init_heap(&mut mapper, &mut frame_allocator);
    fs::init();
    serial_println!("AryOS boot: paging and heap initialized");

    if let Some(framebuffer) = boot_info.framebuffer.as_mut() {
        vga::init(framebuffer);
        vga::splash();
        vga::init(framebuffer);
        vga::println("ARYOS v1.0");
        vga::println("System shell ready.");
        vga::println("Type 'help' for commands.");
    }

    let mut shell = shell::Shell::new();
    let mut keyboard = task::keyboard();
    let mut last_status = 0u64;

    serial_println!("AryOS boot: entering main loop");
    loop {
        task::yield_now();

        while let Some(scancode) = task::pop_scancode() {
            if let Some(decoded) = task::decoded_key(&mut keyboard, scancode) {
                match decoded {
                    DecodedKey::Unicode('\n') => shell.execute(),
                    DecodedKey::Unicode('\r') => shell.execute(),
                    DecodedKey::Unicode('\u{8}') => shell.backspace(),
                    DecodedKey::Unicode(character) => {
                        if character.is_ascii_graphic() || character == ' ' {
                            shell.input.push(character);
                            vga::print(&character.to_string());
                        }
                    }
                    DecodedKey::RawKey(pc_keyboard::KeyCode::ArrowUp) => shell.previous_history(),
                    DecodedKey::RawKey(pc_keyboard::KeyCode::ArrowDown) => shell.next_history(),
                    _ => {}
                }
            }
        }

        let seconds = task::uptime_seconds();
        if seconds != last_status {
            last_status = seconds;
            let (s, m, h, d, mo, y) = rtc::read();
            let status = alloc::format!("{:02}:{:02}:{:02}  UP {:02}:{:02}:{:02}",
                h, m, s, seconds / 3600, (seconds / 60) % 60, seconds % 60);
            if let Some(writer) = vga::WRITER.lock().as_mut() {
                writer.draw_status(&status);
            }
            serial_println!("uptime={}s date={:04}-{:02}-{:02}", seconds, y, mo, d);
        }

        x86_64::instructions::interrupts::enable_and_hlt();
    }
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    serial_println!("KERNEL PANIC: {}", info);

    if let Some(writer) = vga::WRITER.lock().as_mut() {
        writer.clear();
        writer.fill_rect(0, 0, writer.info.width, 40, (120, 20, 20));
        writer.draw_text("ARYOS KERNEL PANIC", 20, 12, 2, (255, 230, 230));
        writer.draw_text("System halted safely", 20, 70, 2, (255, 180, 180));
    }

    loop {
        hlt();
    }
}

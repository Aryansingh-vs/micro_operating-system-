

#![no_std]
#![no_main]

use bootloader_api::info::{FrameBuffer, FrameBufferInfo, PixelFormat};
use bootloader_api::{entry_point, BootInfo};
use pc_keyboard::{
    layouts, DecodedKey, HandleControl, KeyState, PS2Keyboard, ScancodeSet1,
};
use x86_64::instructions::port::Port;

entry_point!(kernel_main);

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    let mut keyboard = PS2Keyboard::new(
        ScancodeSet1::new(),
        layouts::Us104Key,
        HandleControl::MapLettersToUnicode,
    );

    let mut status_port = Port::<u8>::new(0x64);
    let mut data_port = Port::<u8>::new(0x60);

    if let Some(framebuffer) = boot_info.framebuffer.as_mut() {
        draw_welcome(framebuffer);

        let info = framebuffer.info().clone();
        let mut cursor_x: usize = 30;
        let cursor_y = info.height / 2 + 100;

        loop {
            let status = unsafe { status_port.read() };

            if status & 1 != 0 {
                let scancode = unsafe { data_port.read() };

                if let Ok(Some(event)) = keyboard.add_byte(scancode) {
                    if event.state == KeyState::Down {
                        if let Some(DecodedKey::Unicode(character)) =
                            keyboard.process_keyevent(event)
                        {
                            if character == '\n' {
                                cursor_x = 30;
                            } else if character == '\u{8}' {
                                cursor_x = cursor_x.saturating_sub(24);
                            } else if character.is_ascii_graphic()
                                || character == ' '
                            {
                                draw_char(
                                    framebuffer.buffer_mut(),
                                    &info,
                                    character as u8,
                                    cursor_x,
                                    cursor_y,
                                    3,
                                    (255, 255, 255),
                                );

                                cursor_x += 24;

                                if cursor_x + 24 >= info.width {
                                    cursor_x = 30;
                                }
                            }
                        }
                    }
                }
            }

            core::hint::spin_loop();
        }
    }

    loop {
        core::hint::spin_loop();
    }
}

fn draw_welcome(framebuffer: &mut FrameBuffer) {
    let info = framebuffer.info().clone();
    let buffer = framebuffer.buffer_mut();

    fill(buffer, &info, (12, 18, 38));

    let scale = 8;
    let width = text_width("ARYOS", scale);

    draw_text(
        buffer,
        &info,
        "ARYOS",
        info.width.saturating_sub(width) / 2,
        info.height / 2 - 70,
        scale,
        (80, 220, 255),
    );

    let scale = 4;
    let width = text_width("WELCOME", scale);

    draw_text(
        buffer,
        &info,
        "WELCOME",
        info.width.saturating_sub(width) / 2,
        info.height / 2 + 30,
        scale,
        (235, 240, 255),
    );
}

fn text_width(text: &str, scale: usize) -> usize {
    text.len().saturating_mul(6 * scale).saturating_sub(scale)
}

fn draw_text(
    buffer: &mut [u8],
    info: &FrameBufferInfo,
    text: &str,
    x: usize,
    y: usize,
    scale: usize,
    color: (u8, u8, u8),
) {
    for (index, character) in text.bytes().enumerate() {
        draw_char(
            buffer,
            info,
            character,
            x + index * 6 * scale,
            y,
            scale,
            color,
        );
    }
}

fn draw_char(
    buffer: &mut [u8],
    info: &FrameBufferInfo,
    character: u8,
    x: usize,
    y: usize,
    scale: usize,
    color: (u8, u8, u8),
) {
    for (row_index, row) in glyph(character).iter().enumerate() {
        for column in 0..5 {
            if row & (1 << (4 - column)) != 0 {
                for dy in 0..scale {
                    for dx in 0..scale {
                        set_pixel(
                            buffer,
                            info,
                            x + column * scale + dx,
                            y + row_index * scale + dy,
                            color,
                        );
                    }
                }
            }
        }
    }
}

fn fill(buffer: &mut [u8], info: &FrameBufferInfo, color: (u8, u8, u8)) {
    for y in 0..info.height {
        for x in 0..info.width {
            set_pixel(buffer, info, x, y, color);
        }
    }
}

fn set_pixel(
    buffer: &mut [u8],
    info: &FrameBufferInfo,
    x: usize,
    y: usize,
    (red, green, blue): (u8, u8, u8),
) {
    if x >= info.width || y >= info.height {
        return;
    }

    let offset = (y * info.stride + x) * info.bytes_per_pixel;

    if offset + info.bytes_per_pixel > buffer.len() {
        return;
    }

    let channels = match info.pixel_format {
        PixelFormat::Rgb => [red, green, blue],
        PixelFormat::Bgr => [blue, green, red],
        PixelFormat::U8 => {
            let gray =
                ((u16::from(red) * 30 + u16::from(green) * 59
                    + u16::from(blue) * 11) / 100) as u8;
            [gray, gray, gray]
        }
        PixelFormat::Unknown {
            red_position,
            green_position,
            blue_position,
        } => {
            let mut channels = [0; 3];

            for (position, value) in [
                (red_position, red),
                (green_position, green),
                (blue_position, blue),
            ] {
                let byte = usize::from(position / 8);

                if byte < channels.len() {
                    channels[byte] = value;
                }
            }

            channels
        }
        _ => [red, green, blue],
    };

    for (index, channel) in channels.iter().enumerate() {
        if index < info.bytes_per_pixel {
            buffer[offset + index] = *channel;
        }
    }
}

fn glyph(character: u8) -> [u8; 7] {
    match character.to_ascii_uppercase() {
        b'A' => [14, 17, 17, 31, 17, 17, 17],
        b'B' => [30, 17, 17, 30, 17, 17, 30],
        b'C' => [14, 17, 16, 16, 16, 17, 14],
        b'D' => [30, 17, 17, 17, 17, 17, 30],
        b'E' => [31, 16, 16, 30, 16, 16, 31],
        b'F' => [31, 16, 16, 30, 16, 16, 16],
        b'G' => [14, 17, 16, 23, 17, 17, 15],
        b'H' => [17, 17, 17, 31, 17, 17, 17],
        b'I' => [14, 4, 4, 4, 4, 4, 14],
        b'J' => [7, 2, 2, 2, 18, 18, 12],
        b'K' => [17, 18, 20, 24, 20, 18, 17],
        b'L' => [16, 16, 16, 16, 16, 16, 31],
        b'M' => [17, 27, 21, 21, 17, 17, 17],
        b'N' => [17, 25, 25, 21, 19, 19, 17],
        b'O' => [14, 17, 17, 17, 17, 17, 14],
        b'P' => [30, 17, 17, 30, 16, 16, 16],
        b'Q' => [14, 17, 17, 17, 21, 18, 13],
        b'R' => [30, 17, 17, 30, 20, 18, 17],
        b'S' => [15, 16, 16, 14, 1, 1, 30],
        b'T' => [31, 4, 4, 4, 4, 4, 4],
        b'U' => [17, 17, 17, 17, 17, 17, 14],
        b'V' => [17, 17, 17, 17, 17, 10, 4],
        b'W' => [17, 17, 17, 21, 21, 21, 10],
        b'X' => [17, 17, 10, 4, 10, 17, 17],
        b'Y' => [17, 17, 10, 4, 4, 4, 4],
        b'Z' => [31, 1, 2, 4, 8, 16, 31],
        b'0' => [14, 17, 19, 21, 25, 17, 14],
        b'1' => [4, 12, 4, 4, 4, 4, 14],
        b'2' => [14, 17, 1, 2, 4, 8, 31],
        b'3' => [30, 1, 1, 14, 1, 1, 30],
        b'4' => [2, 6, 10, 18, 31, 2, 2],
        b'5' => [31, 16, 16, 30, 1, 1, 30],
        b'6' => [14, 16, 16, 30, 17, 17, 14],
        b'7' => [31, 1, 2, 4, 8, 8, 8],
        b'8' => [14, 17, 17, 14, 17, 17, 14],
        b'9' => [14, 17, 17, 15, 1, 1, 14],
        _ => [0; 7],
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}



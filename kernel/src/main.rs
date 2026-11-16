#![no_std]
#![no_main]

use bootloader_api::info::{FrameBuffer, FrameBufferInfo, PixelFormat};
use bootloader_api::{BootInfo, entry_point};

entry_point!(kernel_main);

fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    if let Some(framebuffer) = boot_info.framebuffer.as_mut() {
        draw_welcome(framebuffer);
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
    for (character_index, character) in text.bytes().enumerate() {
        for (row_index, row) in glyph(character).iter().enumerate() {
            for column in 0..5 {
                if row & (1 << (4 - column)) != 0 {
                    for dy in 0..scale {
                        for dx in 0..scale {
                            set_pixel(
                                buffer,
                                info,
                                x + character_index * 6 * scale + column * scale + dx,
                                y + row_index * scale + dy,
                                color,
                            );
                        }
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
                ((u16::from(red) * 30 + u16::from(green) * 59 + u16::from(blue) * 11) / 100) as u8;
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
    match character {
        b'A' => [14, 17, 17, 31, 17, 17, 17],
        b'C' => [14, 17, 16, 16, 16, 17, 14],
        b'E' => [31, 16, 16, 30, 16, 16, 31],
        b'L' => [16, 16, 16, 16, 16, 16, 31],
        b'M' => [17, 27, 21, 21, 17, 17, 17],
        b'O' => [14, 17, 17, 17, 17, 17, 14],
        b'R' => [30, 17, 17, 30, 20, 18, 17],
        b'S' => [15, 16, 16, 14, 1, 1, 30],
        b'W' => [17, 17, 17, 21, 21, 21, 10],
        b'Y' => [17, 17, 10, 4, 4, 4, 4],
        _ => [0; 7],
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {
        core::hint::spin_loop();
    }
}



#![no_std]
#![no_main]

mod console;

use bootloader_api::info::{FrameBuffer, FrameBufferInfo, PixelFormat};
use bootloader_api::{entry_point, BootInfo};

use pc_keyboard::{
    layouts, DecodedKey, HandleControl, KeyState, PS2Keyboard, ScancodeSet1,
};

use x86_64::instructions::port::Port;
use console::Console;

entry_point!(kernel_main);

// AryOS theme
const BG: (u8, u8, u8) = (9, 14, 30);
const PANEL: (u8, u8, u8) = (17, 26, 48);
const BORDER: (u8, u8, u8) = (40, 65, 100);
const FG: (u8, u8, u8) = (225, 235, 250);
const CYAN: (u8, u8, u8) = (65, 220, 255);

const START_X: usize = 55;
const START_Y: usize = 220;

const CELL_WIDTH: usize = 12;
const CELL_HEIGHT: usize = 16;
const FONT_SCALE: usize = 2;

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
        let info = framebuffer.info().clone();
        let mut console = Console::new();

        draw_background(framebuffer, &info);

        loop {
            let status = unsafe { status_port.read() };

            if status & 1 != 0 {
                let scancode = unsafe { data_port.read() };

                if let Ok(Some(event)) = keyboard.add_byte(scancode) {
                    if event.state == KeyState::Down {
                        if let Some(DecodedKey::Unicode(character)) =
                            keyboard.process_keyevent(event)
                        {
                            let old_row = console.row;
                            let old_column = console.column;

                            if character == '\n' {
                                console.write_byte(b'\n');

                                // Redraw console rows to handle scrolling.
                                for row in 0..console::HEIGHT {
                                    draw_console_row(
                                        framebuffer,
                                        &info,
                                        &console,
                                        row,
                                    );
                                }
                            } else if character == '\u{8}' {
                                console.write_byte(8);

                                if old_row != console.row
                                    || old_column != console.column
                                {
                                    draw_console_cell(
                                        framebuffer,
                                        &info,
                                        &console,
                                        console.row,
                                        console.column,
                                    );
                                }
                            } else if character.is_ascii_graphic()
                                || character == ' '
                            {
                                console.write_byte(character as u8);

                                let new_row = console.row;
                                let new_column = console.column - 1;

                                draw_console_cell(
                                    framebuffer,
                                    &info,
                                    &console,
                                    new_row,
                                    new_column,
                                );
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

// Draw the main AryOS interface.
fn draw_background(
    framebuffer: &mut FrameBuffer,
    info: &FrameBufferInfo,
) {
    let buffer = framebuffer.buffer_mut();

    fill(buffer, info, BG);

    // Main branding
    draw_text(
        buffer,
        info,
        "ARYOS",
        55,
        35,
        5,
        CYAN,
    );

    draw_text(
        buffer,
        info,
        "SYSTEM CONSOLE",
        60,
        105,
        2,
        FG,
    );

    // Main terminal panel
    fill_rect(
        buffer,
        info,
        35,
        155,
        1062,
        500,
        PANEL,
    );

    // Panel borders
    fill_rect(buffer, info, 35, 155, 1062, 2, BORDER);
    fill_rect(buffer, info, 35, 653, 1062, 2, BORDER);
    fill_rect(buffer, info, 35, 155, 2, 500, BORDER);
    fill_rect(buffer, info, 1095, 155, 2, 500, BORDER);

    // Terminal title
    draw_text(
        buffer,
        info,
        "TERMINAL",
        55,
        175,
        2,
        CYAN,
    );

    // Separator
    fill_rect(
        buffer,
        info,
        55,
        202,
        1020,
        1,
        BORDER,
    );

    // Footer
    draw_text(
        buffer,
        info,
        "ARYOS READY",
        55,
        690,
        2,
        CYAN,
    );
}

// Draw one character cell.
fn draw_console_cell(
    framebuffer: &mut FrameBuffer,
    info: &FrameBufferInfo,
    console: &Console,
    row: usize,
    column: usize,
) {
    if row >= console::HEIGHT || column >= console::WIDTH {
        return;
    }

    let x = START_X + column * CELL_WIDTH;
    let y = START_Y + row * CELL_HEIGHT;

    let buffer = framebuffer.buffer_mut();

    // Clear only the changed character area.
    fill_rect(
        buffer,
        info,
        x,
        y,
        CELL_WIDTH,
        CELL_HEIGHT,
        PANEL,
    );

    let character = console.get_buffer()[row][column];

    draw_char(
        buffer,
        info,
        character,
        x,
        y,
        FONT_SCALE,
        FG,
    );
}

// Redraw one console row.
fn draw_console_row(
    framebuffer: &mut FrameBuffer,
    info: &FrameBufferInfo,
    console: &Console,
    row: usize,
) {
    if row >= console::HEIGHT {
        return;
    }

    for column in 0..console::WIDTH {
        draw_console_cell(
            framebuffer,
            info,
            console,
            row,
            column,
        );
    }
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

fn fill(
    buffer: &mut [u8],
    info: &FrameBufferInfo,
    color: (u8, u8, u8),
) {
    for y in 0..info.height {
        for x in 0..info.width {
            set_pixel(buffer, info, x, y, color);
        }
    }
}

fn fill_rect(
    buffer: &mut [u8],
    info: &FrameBufferInfo,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
    color: (u8, u8, u8),
) {
    for dy in 0..height {
        for dx in 0..width {
            set_pixel(
                buffer,
                info,
                x + dx,
                y + dy,
                color,
            );
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

    let offset =
        (y * info.stride + x) * info.bytes_per_pixel;

    if offset + info.bytes_per_pixel > buffer.len() {
        return;
    }

    let channels = match info.pixel_format {
        PixelFormat::Rgb => [red, green, blue],

        PixelFormat::Bgr => [blue, green, red],

        PixelFormat::U8 => {
            let gray =
                ((u16::from(red) * 30
                    + u16::from(green) * 59
                    + u16::from(blue) * 11)
                    / 100) as u8;

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



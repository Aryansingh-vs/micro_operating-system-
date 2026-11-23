use bootloader_api::info::{FrameBuffer, FrameBufferInfo, PixelFormat};
use spin::Mutex;

pub const WIDTH: usize = 80;
pub const HEIGHT: usize = 25;

const BG: (u8, u8, u8) = (9, 14, 30);
const PANEL: (u8, u8, u8) = (17, 26, 48);
const BORDER: (u8, u8, u8) = (40, 65, 100);
const FG: (u8, u8, u8) = (225, 235, 250);
const CYAN: (u8, u8, u8) = (65, 220, 255);

pub struct VgaWriter {
    base: *mut u8,
    len: usize,
    pub info: FrameBufferInfo,
    buffer: [[u8; WIDTH]; HEIGHT],
    row: usize,
    column: usize,
}

unsafe impl Send for VgaWriter {}
unsafe impl Sync for VgaWriter {}

impl VgaWriter {
    pub fn clear(&mut self) {
        self.buffer = [[b' '; WIDTH]; HEIGHT];
        self.row = 0;
        self.column = 0;
        self.fill(BG);
    }

    pub fn put_char(&mut self, byte: u8) {
        match byte {
            b'\n' => {
                self.column = 0;
                if self.row + 1 < HEIGHT {
                    self.row += 1;
                } else {
                    self.scroll();
                }
            }
            8 => {
                if self.column > 0 {
                    self.column -= 1;
                    self.buffer[self.row][self.column] = b' ';
                    self.draw_cell(self.row, self.column);
                }
            }
            b' '..=b'~' => {
                if self.column >= WIDTH {
                    self.column = 0;
                    if self.row + 1 < HEIGHT { self.row += 1; } else { self.scroll(); }
                }
                self.buffer[self.row][self.column] = byte;
                self.draw_cell(self.row, self.column);
                self.column += 1;
            }
            _ => {}
        }
    }

    pub fn write_str(&mut self, text: &str) {
        for byte in text.bytes() {
            self.put_char(byte);
        }
    }

    fn scroll(&mut self) {
        for row in 1..HEIGHT {
            self.buffer[row - 1] = self.buffer[row];
        }
        self.buffer[HEIGHT - 1] = [b' '; WIDTH];
        self.row = HEIGHT - 1;
        for row in 0..HEIGHT {
            for column in 0..WIDTH { self.draw_cell(row, column); }
        }
    }

    pub fn draw_desktop(&mut self) {
        self.fill(BG);
        self.fill_rect(0, 0, self.info.width, 32, (31, 78, 121));
        self.draw_text("ARYOS  DESKTOP", 14, 8, 2, CYAN);

        let (window_x, window_y) = (20, 72);
        let (window_w, window_h) = (780, 510);
        self.fill_rect(window_x, window_y, window_w, window_h, PANEL);
        self.fill_rect(window_x, window_y, window_w, 3, BORDER);
        self.fill_rect(window_x, window_y + window_h - 3, window_w, 3, BORDER);
        self.fill_rect(window_x, window_y, 3, window_h, BORDER);
        self.fill_rect(window_x + window_w - 3, window_y, 3, window_h, BORDER);
        self.draw_text("ARYOS SHELL", window_x + 12, window_y + 10, 2, CYAN);

        self.fill_rect(0, self.info.height - 30, self.info.width, 30, (20, 38, 68));
        self.draw_text("AryOS", 10, self.info.height - 24, 2, CYAN);
        self.draw_text("Keyboard: READY", 150, self.info.height - 24, 2, FG);
    }

    pub fn draw_status(&mut self, text: &str) {
        let width = text.len().min(40);
        let x = self.info.width.saturating_sub(width * 12 + 20);
        self.fill_rect(x, 4, width * 12 + 10, 24, (31, 78, 121));
        self.draw_text(text, x + 5, 8, 2, FG);
    }

    pub fn set_cursor(&mut self, row: usize, column: usize) {
        self.row = row;
        self.column = column;
    }

    fn draw_cell(&mut self, row: usize, column: usize) {
        if row >= HEIGHT || column >= WIDTH { return; }
        let x = 26 + column * 10;
        let y = 110 + row * 16;
        self.fill_rect(x, y, 10, 16, PANEL);
        let ch = self.buffer[row][column];
        self.draw_char(ch, x, y, 2, FG);
    }

    fn fill(&mut self, color: (u8, u8, u8)) {
        self.fill_rect(0, 0, self.info.width, self.info.height, color);
    }

    pub fn fill_rect(&mut self, x: usize, y: usize, width: usize, height: usize, color: (u8, u8, u8)) {
        for dy in 0..height {
            for dx in 0..width {
                self.set_pixel(x + dx, y + dy, color);
            }
        }
    }

    pub fn draw_text(&mut self, text: &str, x: usize, y: usize, scale: usize, color: (u8, u8, u8)) {
        for (i, byte) in text.bytes().enumerate() {
            self.draw_char(byte, x + i * 6 * scale, y, scale, color);
        }
    }

    fn draw_char(&mut self, character: u8, x: usize, y: usize, scale: usize, color: (u8, u8, u8)) {
        for (row, pattern) in glyph(character).iter().enumerate() {
            for column in 0..5 {
                if pattern & (1 << (4 - column)) != 0 {
                    for dy in 0..scale {
                        for dx in 0..scale {
                            self.set_pixel(x + column * scale + dx, y + row * scale + dy, color);
                        }
                    }
                }
            }
        }
    }

    fn set_pixel(&mut self, x: usize, y: usize, (r, g, b): (u8, u8, u8)) {
        if x >= self.info.width || y >= self.info.height { return; }
        let offset = (y * self.info.stride + x) * self.info.bytes_per_pixel;
        unsafe {
            if offset + self.info.bytes_per_pixel > self.len { return; }
            let ptr = self.base.add(offset);
            match self.info.pixel_format {
                PixelFormat::Rgb => {
                    if self.info.bytes_per_pixel >= 3 {
                        ptr.write(r); ptr.add(1).write(g); ptr.add(2).write(b);
                    }
                }
                PixelFormat::Bgr => {
                    if self.info.bytes_per_pixel >= 3 {
                        ptr.write(b); ptr.add(1).write(g); ptr.add(2).write(r);
                    }
                }
                PixelFormat::U8 => ptr.write(((r as u16 * 30 + g as u16 * 59 + b as u16 * 11) / 100) as u8),
                _ => {}
            }
        }
    }
}

pub static WRITER: Mutex<Option<VgaWriter>> = Mutex::new(None);

pub fn init(framebuffer: &mut FrameBuffer) {
    let info = framebuffer.info().clone();
    let len = framebuffer.buffer().len();
    let base = framebuffer.buffer_mut().as_mut_ptr();
    let mut writer = VgaWriter {
        base, len, info,
        buffer: [[b' '; WIDTH]; HEIGHT],
        row: 0,
        column: 0,
    };
    writer.draw_desktop();
    writer.set_cursor(3, 2);
    *WRITER.lock() = Some(writer);
}

pub fn print(text: &str) {
    crate::serial_print!("{}", text);
    if let Some(writer) = WRITER.lock().as_mut() {
        writer.write_str(text);
    }
}

pub fn println(text: &str) {
    print(text);
    crate::serial_println!();
    if let Some(writer) = WRITER.lock().as_mut() {
        writer.put_char(b'\n');
    }
}

pub fn splash() {
    if let Some(writer) = WRITER.lock().as_mut() {
        writer.fill((9, 14, 30));
        writer.draw_text("ARYOS", 150, 180, 5, CYAN);
        writer.draw_text("Rust Operating System", 160, 260, 2, FG);
        for i in 0..=40 {
            let progress = i * 10;
            writer.fill_rect(160, 330, 400, 20, (17, 26, 48));
            writer.fill_rect(160, 330, progress * 10 / 40, 20, CYAN);
            for _ in 0..200_000 { core::hint::spin_loop(); }
        }
    }
}

fn glyph(character: u8) -> [u8; 7] {
    match character.to_ascii_uppercase() {
        b'A' => [14,17,17,31,17,17,17], b'B' => [30,17,17,30,17,17,30],
        b'C' => [14,17,16,16,16,17,14], b'D' => [30,17,17,17,17,17,30],
        b'E' => [31,16,16,30,16,16,31], b'F' => [31,16,16,30,16,16,16],
        b'G' => [14,17,16,23,17,17,15], b'H' => [17,17,17,31,17,17,17],
        b'I' => [14,4,4,4,4,4,14], b'J' => [7,2,2,2,18,18,12],
        b'K' => [17,18,20,24,20,18,17], b'L' => [16,16,16,16,16,16,31],
        b'M' => [17,27,21,21,17,17,17], b'N' => [17,25,25,21,19,19,17],
        b'O' => [14,17,17,17,17,17,14], b'P' => [30,17,17,30,16,16,16],
        b'R' => [30,17,17,30,20,18,17], b'S' => [15,16,16,14,1,1,30],
        b'T' => [31,4,4,4,4,4,4], b'U' => [17,17,17,17,17,17,14],
        b'V' => [17,17,17,17,17,10,4], b'W' => [17,17,17,21,21,21,10],
        b'X' => [17,17,10,4,10,17,17], b'Y' => [17,17,10,4,4,4,4],
        b'Z' => [31,1,2,4,8,16,31],
        b'0' => [14,17,19,21,25,17,14], b'1' => [4,12,4,4,4,4,14],
        b'2' => [14,17,1,2,4,8,31], b'3' => [30,1,1,14,1,1,30],
        b'4' => [2,6,10,18,31,2,2], b'5' => [31,16,16,30,1,1,30],
        b'6' => [14,16,16,30,17,17,14], b'7' => [31,1,2,4,8,8,8],
        b'8' => [14,17,17,14,17,17,14], b'9' => [14,17,17,15,1,1,14],
        b'_' => [0,0,0,0,0,0,31], b'-' => [0,0,0,31,0,0,0],
        b':' => [0,4,4,0,4,4,0], b'/' => [1,2,2,4,8,8,16],
        b'.' => [0,0,0,0,0,4,4], b'!' => [4,4,4,4,4,0,4],
        b'?' => [14,17,1,2,4,0,4], b'+' => [0,4,4,31,4,4,0],
        b'*' => [0,21,14,31,14,21,0],
        _ => [0; 7],
    }
}

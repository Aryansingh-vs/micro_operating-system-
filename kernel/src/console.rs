

pub const WIDTH: usize = 80;
pub const HEIGHT: usize = 25;

pub struct Console {
    buffer: [[u8; WIDTH]; HEIGHT],
    pub column: usize,
    pub row: usize,
}

impl Console {
    pub const fn new() -> Self {
        Self {
            buffer: [[b' '; WIDTH]; HEIGHT],
            column: 0,
            row: 0,
        }
    }

    pub fn write_byte(&mut self, byte: u8) {
        match byte {
            b'\n' => self.new_line(),

            8 => self.backspace(),

            _ => {
                if self.column >= WIDTH {
                    self.new_line();
                }

                self.buffer[self.row][self.column] = byte;
                self.column += 1;
            }
        }
    }

    fn backspace(&mut self) {
        if self.column > 0 {
            self.column -= 1;
        } else if self.row > 0 {
            self.row -= 1;

            self.column = WIDTH;

            while self.column > 0
                && self.buffer[self.row][self.column - 1] == b' '
            {
                self.column -= 1;
            }

            if self.column == 0 {
                return;
            }

            self.column -= 1;
        } else {
            return;
        }

        self.buffer[self.row][self.column] = b' ';
    }

    fn new_line(&mut self) {
        self.column = 0;

        if self.row + 1 < HEIGHT {
            self.row += 1;
        } else {
            self.scroll();
        }
    }

    fn scroll(&mut self) {
        for row in 1..HEIGHT {
            self.buffer[row - 1] = self.buffer[row];
        }

        self.buffer[HEIGHT - 1] = [b' '; WIDTH];
        self.row = HEIGHT - 1;
    }

    pub fn clear(&mut self) {
        self.buffer = [[b' '; WIDTH]; HEIGHT];
        self.column = 0;
        self.row = 0;
    }

    pub fn get_buffer(&self) -> &[[u8; WIDTH]; HEIGHT] {
        &self.buffer
    }
}


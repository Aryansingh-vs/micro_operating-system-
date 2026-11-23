use x86_64::instructions::port::Port;

fn read_cmos(register: u8) -> u8 {
    let mut address = Port::<u8>::new(0x70);
    let mut data = Port::<u8>::new(0x71);
    unsafe {
        address.write(register);
        data.read()
    }
}

fn bcd(byte: u8) -> u8 {
    ((byte >> 4) * 10) + (byte & 0x0f)
}

pub fn read() -> (u8, u8, u8, u8, u8, u16) {
    let seconds = read_cmos(0);
    let minutes = read_cmos(2);
    let hours = read_cmos(4);
    let day = read_cmos(7);
    let month = read_cmos(8);
    let year = bcd(read_cmos(9)) as u16 + 2000;

    let status_b = read_cmos(0x0b);
    if status_b & 4 == 0 {
        (bcd(seconds), bcd(minutes), bcd(hours), bcd(day), bcd(month), year)
    } else {
        (seconds, minutes, hours, bcd(day), bcd(month), year)
    }
}

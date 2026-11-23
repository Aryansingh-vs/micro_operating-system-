use spin::Mutex;
use uart_16550::SerialPort;

/// COM1 serial output is useful in both QEMU and VirtualBox when a serial log is attached.
pub static SERIAL1: Mutex<Option<SerialPort>> = Mutex::new(None);

pub fn init() {
    let mut serial = unsafe { SerialPort::new(0x3F8) };
    serial.init();
    *SERIAL1.lock() = Some(serial);
}

#[doc(hidden)]
pub fn _print(args: core::fmt::Arguments) {
    use core::fmt::Write;
    if let Some(serial) = SERIAL1.lock().as_mut() {
        let _ = serial.write_fmt(args);
    }
}

#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => ($crate::serial::_print(format_args!($($arg)*)));
}

#[macro_export]
macro_rules! serial_println {
    () => ($crate::serial_print!("\n"));
    ($fmt:expr) => ($crate::serial_print!(concat!($fmt, "\n")));
    ($fmt:expr, $($arg:tt)*) => ($crate::serial_print!(concat!($fmt, "\n"), $($arg)*));
}

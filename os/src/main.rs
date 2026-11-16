use std::env;
use std::process::Command;

fn main() {
    let mode = env::args().nth(1).unwrap_or_else(|| {
        eprintln!("Usage: cargo run -p aryos -- [bios|uefi]");
        std::process::exit(2);
    });

    let mut qemu = Command::new("qemu-system-x86_64");
    qemu.arg("-m").arg("256M").arg("-no-reboot").arg("-drive");

    match mode.as_str() {
        "bios" => {
            qemu.arg(format!("format=raw,file={}", env!("ARYOS_BIOS_IMAGE")));
        }
        "uefi" => {
            eprintln!("UEFI QEMU launch is added after the BIOS boot milestone.");
            std::process::exit(2);
        }
        _ => {
            eprintln!("Unknown mode: {mode}. Use bios or uefi.");
            std::process::exit(2);
        }
    }

    let status = qemu.status().expect("start qemu-system-x86_64");
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
}

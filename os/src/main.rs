use std::env;
use std::process::Command;

fn main() {
    let mode = env::args().nth(1).unwrap_or_else(|| {
        eprintln!("Usage: cargo run -p aryos -- [image|bios]");
        std::process::exit(2);
    });

    match mode.as_str() {
        "image" => {
            let output_dir = std::path::Path::new("dist");
            std::fs::create_dir_all(output_dir).expect("create dist directory");
            let output = output_dir.join("aryos-uefi.img");
            std::fs::copy(env!("ARYOS_UEFI_IMAGE"), &output)
                .expect("copy UEFI disk image to dist/");
            println!("UEFI disk image ready: {}", output.display());
        }
        "bios" => {
            let mut qemu = Command::new("qemu-system-x86_64");
            qemu.arg("-m").arg("256M").arg("-no-reboot").arg("-drive");
            qemu.arg(format!("format=raw,file={}", env!("ARYOS_BIOS_IMAGE")));
            let status = qemu.status().expect("start qemu-system-x86_64");
            if !status.success() {
                std::process::exit(status.code().unwrap_or(1));
            }
        }
        _ => {
            eprintln!("Unknown mode: {mode}. Use image or bios.");
            std::process::exit(2);
        }
    }
}

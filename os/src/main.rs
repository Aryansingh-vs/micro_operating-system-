use std::env;
use std::process::Command;

fn main() {
    let mode = env::args().nth(1).unwrap_or_else(|| {
        "virtualbox".to_string()
    });

    match mode.as_str() {
        "image" => {
            let output_dir = std::path::Path::new("dist");
            std::fs::create_dir_all(output_dir).expect("create dist directory");
            let uefi_output = output_dir.join("aryos-uefi.img");
            let bios_output = output_dir.join("aryos-bios.img");
            std::fs::copy(env!("ARYOS_UEFI_IMAGE"), &uefi_output)
                .expect("copy UEFI disk image to dist/");
            std::fs::copy(env!("ARYOS_BIOS_IMAGE"), &bios_output)
                .expect("copy BIOS disk image to dist/");
            println!("UEFI disk image ready: {}", uefi_output.display());
            println!("BIOS disk image ready: {}", bios_output.display());
        }
        "virtualbox" => {
            let mut virtualbox = Command::new("VBoxManage");
            virtualbox.arg("startvm").arg("AryOS-ARM64").arg("--type").arg("gui");
            let status = virtualbox.status().expect("start VirtualBox");
            if !status.success() {
                std::process::exit(status.code().unwrap_or(1));
            }
        }
        _ => {
            eprintln!("Unknown mode: {mode}. Use image or virtualbox.");
            std::process::exit(2);
        }
    }
}

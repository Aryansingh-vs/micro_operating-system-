use std::path::PathBuf;

fn main() {
    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR"));
    let kernel = PathBuf::from(
        std::env::var_os("CARGO_BIN_FILE_KERNEL_kernel").expect("kernel artifact dependency"),
    );

    let bios_image = out_dir.join("aryos-bios.img");
    bootloader::BiosBoot::new(&kernel)
        .create_disk_image(&bios_image)
        .expect("create BIOS disk image");

    let uefi_image = out_dir.join("aryos-uefi.img");
    bootloader::UefiBoot::new(&kernel)
        .create_disk_image(&uefi_image)
        .expect("create UEFI disk image");

    println!("cargo:rustc-env=ARYOS_BIOS_IMAGE={}", bios_image.display());
    println!("cargo:rustc-env=ARYOS_UEFI_IMAGE={}", uefi_image.display());
}

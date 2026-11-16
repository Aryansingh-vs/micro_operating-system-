# AryOS

AryOS is an independent Rust operating system project targeting x86_64. Its bare-metal `no_std` kernel is loaded by the Rust OSDev bootloader. The development host is Apple Silicon (ARM64), while the first AryOS target is a separate x86_64 PC.

## Current milestone

The kernel draws an AryOS welcome screen using the bootloader-provided framebuffer, then waits. The image builder creates BIOS and UEFI disk images. The UEFI image is intended for compatible x86_64 PCs; it has not yet been tested on physical hardware. This early kernel does not yet provide input, storage, networking, or other general-purpose OS features.

## Requirements (macOS)

- Rust nightly via rustup
- `llvm-tools-preview` and target `x86_64-unknown-none` (selected by `rust-toolchain.toml`)
- QEMU with `qemu-system-x86_64` only if you want the optional BIOS emulator run

Verify the Rust toolchain:

```sh
rustc --version
cargo --version
rustup show active-toolchain
rustup target list --installed
```

## Build an image for an x86_64 PC

From the AryOS repository root:

```sh
cargo build -p aryos
cargo run -p aryos -- image
```

This writes `dist/aryos-uefi.img`. It is a raw UEFI boot disk image for a compatible x86_64 PC. The Apple Silicon Mac itself cannot boot this x86_64 image; an ARM64 port would be a separate project. Secure Boot must be disabled because the AryOS bootloader is not signed.

To make a USB installer, identify the USB device carefully with `diskutil list`, then write the image to that device. Writing a disk image erases the selected drive. Do not run a write command until you have confirmed the device identifier and that the drive contains no needed data. Boot the USB on an x86_64 PC using its UEFI boot menu.

The optional BIOS emulator check is `cargo run -p aryos -- bios`; it is not needed to build the USB image.

## Project layout

- `kernel/`: bare-metal x86_64 kernel (`no_std`, `no_main`)
- `os/`: host-side image builder and optional QEMU launcher
- `.cargo/config.toml`: enables Cargo nightly artifact dependencies
- `rust-toolchain.toml`: selects nightly and the kernel target

## Architecture direction

The bootable kernel currently targets x86_64. Host-side tools run natively on Apple Silicon. Architecture-specific kernel code should remain isolated so an ARM64 target can be explored separately later.

## GitHub and licensing

The repository does not yet select a project license. Choose one before publishing the source under a license. No GitHub repository has been created or pushed from this workspace.

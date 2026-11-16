# AryOS

AryOS is an independent Rust operating system project targeting x86_64. The first milestone boots a `no_std` kernel in QEMU using the Rust OSDev bootloader. The development host in this project is Apple Silicon (ARM64); QEMU emulates the x86_64 guest.

## Current milestone

The kernel draws an AryOS welcome screen using the framebuffer supplied by the bootloader. The image builder creates BIOS and UEFI images. The initial QEMU run command below exercises the BIOS image; UEFI testing follows after the first successful boot.

## Requirements (macOS)

- Rust nightly via rustup
- `llvm-tools-preview` and target `x86_64-unknown-none` (selected by `rust-toolchain.toml`)
- QEMU with `qemu-system-x86_64` (for example, `brew install qemu`)

Verify the tools:

```sh
rustc --version
cargo --version
rustup show active-toolchain
rustup target list --installed
qemu-system-x86_64 --version
```

## Build and boot in QEMU

From the AryOS repository root:

```sh
cargo build -p aryos
cargo run -p aryos -- bios
```

The build creates disk images in Cargo's generated `target/` directory. Close the QEMU window to stop the virtual machine.

## Project layout

- `kernel/`: bare-metal x86_64 kernel (`no_std`, `no_main`)
- `os/`: host-side image builder and QEMU launcher
- `.cargo/config.toml`: enables Cargo nightly artifact dependencies
- `rust-toolchain.toml`: selects nightly and the kernel target

## Architecture direction

The bootable kernel currently targets x86_64. Host-side tools run natively on Apple Silicon. Architecture-specific kernel code should remain isolated so an ARM64 target can be explored separately later.

## GitHub and licensing

The repository does not yet select a project license. Choose one before publishing the source under a license. No GitHub repository has been created or pushed from this workspace.

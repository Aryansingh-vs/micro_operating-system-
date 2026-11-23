# AryOS

AryOS is a small 64-bit operating system written in Rust. It boots as a bare-metal
`no_std` x86_64 kernel, provides a colorful framebuffer desktop, and includes an
in-memory shell and filesystem.

## Features

- Bootloader-provided framebuffer, COM1 serial logging, formatted output, and panic screen
- GDT, IDT, double-fault handler, 8259 PIC, timer, and PS/2 keyboard interrupts
- Physical frame allocator from the boot memory map, paging, and 1 MB kernel heap
- 80x25 text-style desktop, title bar, taskbar, splash screen, live RTC clock, and uptime
- Shell history and commands: `help`, `clear`, `echo`, `ls`, `cat`, `touch`, `write`,
  `rm`, `mkdir`, `mem`, `uptime`, `date`, `sysinfo`, `color`, `reboot`, `shutdown`, `dmesg`
- Heap-backed in-memory directory tree with demo files
- Cooperative keyboard, timer/clock, and shell jobs
- Shell apps: text-file workflow, calculator, monitor, and simple app launch messages
- BIOS and UEFI disk-image generation

## Architecture

```text
+-------------------------------------------------------------+
|                        AryOS shell/UI                       |
|  desktop  |  shell history  |  apps  |  in-memory filesystem|
+-------------------------------------------------------------+
|  task.rs  |  fs.rs  |  apps.rs  |  rtc.rs  |  shell.rs     |
+-------------------------------------------------------------+
|   timer and keyboard IRQ1  |  PIC  |  GDT  |  IDT          |
+-------------------------------------------------------------+
|            paging  |  frame allocator  |  heap              |
+-------------------------------------------------------------+
|  VGA/framebuffer  |  serial COM1  |  bootloader BootInfo    |
+-------------------------------------------------------------+
|                    Rust bare-metal kernel                    |
+-------------------------------------------------------------+
```

The kernel is loaded by the Rust OSDev bootloader. Interrupts run at ring zero,
timer IRQ0 updates uptime, and keyboard IRQ1 places PS/2 scancodes into a queue.
The main loop cooperatively decodes keyboard input, updates the shell and clock,
and waits with interrupts enabled.

## Requirements

- macOS or Linux host
- Rust nightly with `rust-src` and `llvm-tools-preview`
- QEMU for `run.sh`
- Oracle VirtualBox for `setup-vbox.sh`

```sh
rustup target add x86_64-unknown-none
rustup component add rust-src llvm-tools-preview
```

## Build

```sh
./build.sh
```

This builds the kernel and copies both BIOS and UEFI raw disk images into `dist/`.
It also prints both image sizes.

## Run in QEMU

```sh
./run.sh
```

`run.sh` launches the BIOS image with 128 MB RAM.

Direct commands:

```sh
cargo build -p aryos
cargo run -p aryos -- image
qemu-system-x86_64 -m 128M -no-reboot -drive format=raw,file=dist/aryos-bios.img
```

## Run in VirtualBox

```sh
./setup-vbox.sh
```

The setup script:

1. Builds AryOS.
2. Converts `dist/aryos-bios.img` to `dist/aryos-bios.vdi`.
3. Creates a VM named `AryOS`.
4. Uses **Other/Unknown 64-bit** guest type.
5. Uses 128 MB RAM.
6. Uses BIOS firmware (EFI disabled).
7. Attaches the VDI and starts the VM.

If needed, exact manual settings are:

```text
Type: Other
Version: Unknown 64-bit
Memory: 128 MB
Firmware: BIOS
Storage: IDE
Network: none
Graphics controller: VboxVGA
```

## Try the shell

```text
help
ls
cat welcome.txt
touch notes.txt
write notes.txt AryOS college project
cat notes.txt
mem
uptime
date
sysinfo
calc 12 * 7
monitor
clear
```

Use the up and down arrow keys for command history.

### ARM64 UEFI Command Center commands

| Command | Description |
|---|---|
| `help` | Print grouped command help |
| `about` | Show project description |
| `version` | Show the ARYOS build version |
| `time` | Print UEFI clock time and date |
| `date` | Print UEFI clock date and time |
| `screen` | Print the GOP framebuffer resolution |
| `system` | Print system and network status |
| `wifi` | Print UEFI network link status |
| `firmware` | Print vendor, firmware revision and UEFI spec |
| `sysinfo` | Print the OS summary screen |
| `mem` | Print total, usable and reserved memory with a usage bar |
| `uptime` | Print boot uptime as HH:MM:SS |
| `cpu` | Print ARM64 architecture, exception level and MIDR_EL1 |
| `reboot` | Print a message and perform a cold reset |
| `shutdown` | Print a message and shut down |
| `exit` | Ask for confirmation, then shut down |
| `crash` | Display the panic demo output |
| `history` | Print the last ten commands |
| `color` | Cycle cyan, green, amber and pink accents |
| `calc <expr>` | Evaluate integer addition, subtraction, multiplication or division |
| `ping <host>` | Resolve a hostname and send one IPv4 ICMP echo request through UEFI SNP |
| `banner` | Print a text ARYOS logo |
| `whoami` | Print `root@aryos` |
| `neofetch` | Print a styled system summary |
| `echo <text>` | Print text into Command Center |
| `clear` | Clear Command Center output |

`ping` obtains an IPv4 configuration and DNS server through DHCP, then sends one
ICMP echo request through the firmware's Simple Network Protocol. It requires a
connected VirtualBox adapter with a UEFI SNP driver; unsupported interfaces,
DHCP/DNS failures, and timeouts are reported in the Command Center.
On the current VirtualBox ARM64 setup, the E1000 adapter exposes no SNP handle,
and the VirtIO test adapter did not complete DHCP, so a live ping could not be
verified in that VM. The command reports these real failures rather than
displaying a fabricated reply.

## Project layout

- `kernel/src/vga.rs` - framebuffer text-style UI
- `serial.rs` - COM1 debug output
- `interrupts.rs` / `gdt.rs` - CPU and PIC interrupt setup
- `memory.rs` - frame allocator and heap
- `keyboard.rs` / `task.rs` - keyboard/timer task support
- `fs.rs` - in-memory directory tree
- `shell.rs` - command interpreter
- `apps.rs` - calculator and file-backed app helpers
- `rtc.rs` - CMOS RTC clock
- `ui.rs` - desktop UI module

## How it works

The bootloader creates a framebuffer and passes a memory map to the kernel.
The kernel initializes the GDT and IDT, remaps the PIC, enables timer and
keyboard interrupts, maps the heap, and initializes an allocator. The scheduler
loop is cooperative: timer IRQ0 increments uptime and keyboard IRQ1 enqueues
scancodes. The shell interprets commands and uses an allocator-backed directory
tree. Every file and directory exists only in RAM, so changes do not require a
disk driver and disappear after reboot.

## Viva summary

AryOS demonstrates protected-mode setup, paging, allocators, interrupt handling,
device input, memory organization, filesystem design, and a command interpreter
without an existing operating-system runtime. It is intentionally stable and
small: networking and disk filesystems are omitted rather than risking boot
reliability.

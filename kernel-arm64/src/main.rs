#![no_std]
#![no_main]

mod e1000;

use core::cell::Cell;
use core::fmt::{self, Write};
use smoltcp::iface::{Config as NetConfig, Interface, SocketSet, SocketStorage};
use smoltcp::phy::{Device, DeviceCapabilities, Medium, RxToken, TxToken};
use smoltcp::socket::{dhcpv4, dns, icmp};
use smoltcp::time::Instant;
use smoltcp::wire::{
    DnsQueryType, EthernetAddress, HardwareAddress, Icmpv4Packet, Icmpv4Repr, IpAddress, IpCidr,
    Ipv4Address,
};
use uefi::prelude::*;
use uefi::proto::console::gop::{GraphicsOutput, PixelFormat};
use uefi::proto::console::text::Key;
use uefi::proto::device_path::DevicePath;
use uefi::proto::network::snp::SimpleNetwork;

const MAX_LOG_ROWS: usize = 12;
const MAX_LINE_LENGTH: usize = 80;
const FONT_SCALE: usize = 2;
const LINE_HEIGHT: usize = 26;

enum VirtualLink {
    Up,
    Down,
    Unknown,
    Missing,
}

#[derive(Clone, Copy)]
struct Color(u8, u8, u8);

const BACKGROUND: Color = Color(7, 13, 25);
const HEADER: Color = Color(12, 23, 40);
const PANEL: Color = Color(13, 24, 41);
const PANEL_INNER: Color = Color(8, 16, 29);
const BORDER: Color = Color(31, 52, 76);
const WHITE: Color = Color(225, 235, 247);
const MUTED: Color = Color(122, 145, 169);
const CYAN: Color = Color(39, 211, 224);
const GREEN: Color = Color(92, 220, 157);
const ORANGE: Color = Color(255, 170, 77);
const PINK: Color = Color(255, 126, 182);
const RED: Color = Color(255, 103, 115);

struct Terminal {
    lines: [[u8; MAX_LINE_LENGTH]; MAX_LOG_ROWS],
    lengths: [usize; MAX_LOG_ROWS],
    line_count: usize,
    input: [u8; MAX_LINE_LENGTH],
    input_length: usize,
    screen_width: usize,
    screen_height: usize,
    cursor_visible: bool,
    history: [[u8; MAX_LINE_LENGTH]; 10],
    history_lengths: [usize; 10],
    history_count: usize,
    history_index: Option<usize>,
    uptime_start: u64,
    accent: usize,
    exit_pending: bool,
}

impl Terminal {
    const fn new(screen_width: usize, screen_height: usize) -> Self {
        Self {
            lines: [[0; MAX_LINE_LENGTH]; MAX_LOG_ROWS],
            lengths: [0; MAX_LOG_ROWS],
            line_count: 0,
            input: [0; MAX_LINE_LENGTH],
            input_length: 0,
            screen_width,
            screen_height,
            cursor_visible: true,
            history: [[0; MAX_LINE_LENGTH]; 10],
            history_lengths: [0; 10],
            history_count: 0,
            history_index: None,
            uptime_start: 0,
            accent: 0,
            exit_pending: false,
        }
    }

    fn append(&mut self, text: &[u8]) {
        if self.line_count == MAX_LOG_ROWS {
            self.lines.copy_within(1..MAX_LOG_ROWS, 0);
            self.lengths.copy_within(1..MAX_LOG_ROWS, 0);
            self.line_count -= 1;
        }

        let row = self.line_count;
        let length = text.len().min(MAX_LINE_LENGTH);
        self.lines[row][..length].copy_from_slice(&text[..length]);
        self.lengths[row] = length;
        self.line_count += 1;
    }

    fn submit(&mut self) {
        if self.input_length == 0 {
            return;
        }

        if self.exit_pending {
            let answer = char::from(self.input[0]);
            self.exit_pending = false;
            self.input = [0; MAX_LINE_LENGTH];
            self.input_length = 0;
            if answer == 'y' || answer == 'Y' {
                self.append(b"SHUTTING DOWN...");
                uefi::runtime::reset(uefi::runtime::ResetType::SHUTDOWN, Status::SUCCESS, None);
            } else {
                self.append(b"SHUTDOWN CANCELLED");
            }
            return;
        }

        let input = self.input;
        let input_length = self.input_length;
        self.history_add(&input, input_length);
        let mut entered = [0; MAX_LINE_LENGTH];
        let prompt = b"ARYOS> ";
        entered[..prompt.len()].copy_from_slice(prompt);
        let copy_len = input_length.min(MAX_LINE_LENGTH - prompt.len());
        entered[prompt.len()..prompt.len() + copy_len].copy_from_slice(&input[..copy_len]);
        self.append(&entered[..prompt.len() + copy_len]);

        let command = &input[..input_length];
        if command.eq_ignore_ascii_case(b"clear") {
            self.lines = [[0; MAX_LINE_LENGTH]; MAX_LOG_ROWS];
            self.lengths = [0; MAX_LOG_ROWS];
            self.line_count = 0;
        } else if command.eq_ignore_ascii_case(b"help") {
            self.append(b"--- SYSTEM ---");
            self.append(b"HELP       GROUPED COMMAND REFERENCE");
            self.append(b"SYSINFO    OS, CPU AND MEMORY SUMMARY");
            self.append(b"MEM        MEMORY MAP USAGE");
            self.append(b"UPTIME     TIME SINCE BOOT");
            self.append(b"CPU        CPU ARCHITECTURE DETAILS");
            self.append(b"SYSTEM     SYSTEM STATUS");
            self.append(b"TIME       UEFI CLOCK TIME");
            self.append(b"DATE       UEFI CLOCK DATE");
            self.append(b"SCREEN     FRAMEBUFFER RESOLUTION");
            self.append(b"FIRMWARE   VENDOR AND UEFI REVISION");
            self.append(b"--- POWER ---");
            self.append(b"REBOOT     COLD REBOOT");
            self.append(b"SHUTDOWN   POWER OFF");
            self.append(b"EXIT       CONFIRM AND POWER OFF");
            self.append(b"--- TOOLS ---");
            self.append(b"CALC       INTEGER CALCULATOR");
            self.append(b"PING       DNS AND ICMP ECHO TEST");
            self.append(b"HISTORY    LAST TEN COMMANDS");
            self.append(b"COLOR      CYCLE ACCENT COLOR");
            self.append(b"ECHO       PRINT TEXT");
            self.append(b"CLEAR      CLEAR OUTPUT");
            self.append(b"--- FUN ---");
            self.append(b"BANNER     ARYOS LOGO");
            self.append(b"WHOAMI     CURRENT USER");
            self.append(b"NEOFETCH   SYSTEM SUMMARY");
            self.append(b"ABOUT      ABOUT ARYOS");
            self.append(b"VERSION    BUILD VERSION");
            self.append(b"CRASH      DEMO PANIC SCREEN");
        } else if command.eq_ignore_ascii_case(b"about") {
            self.append(b"ARYOS IS A SMALL OS PROJECT FOR ARM64.");
        } else if command.eq_ignore_ascii_case(b"version") {
            self.append(b"ARYOS ARM64 - DEVELOPMENT BUILD");
        } else if command.eq_ignore_ascii_case(b"time") || command.eq_ignore_ascii_case(b"date") {
            match uefi::runtime::get_time() {
                Ok(time) => self.append_fmt(format_args!(
                    "DATE {:04}-{:02}-{:02}  TIME {:02}:{:02}:{:02}",
                    time.year(),
                    time.month(),
                    time.day(),
                    time.hour(),
                    time.minute(),
                    time.second(),
                )),
                Err(error) => {
                    self.append_fmt(format_args!("CLOCK ERROR: {:?}", error.status()));
                    self.append_uptime();
                }
            }
        } else if command.eq_ignore_ascii_case(b"screen") {
            let (width, height) = (self.screen_width, self.screen_height);
            self.append_fmt(format_args!("SCREEN {} X {} PIXELS", width, height,));
        } else if command.eq_ignore_ascii_case(b"firmware") {
            self.append_firmware();
        } else if command.eq_ignore_ascii_case(b"sysinfo") {
            self.append(b"OS: ARYOS ARM64");
            self.append(b"VERSION: DEVELOPMENT BUILD");
            self.append(b"ARCH: ARM64");
            self.append_firmware();
            let (width, height) = (self.screen_width, self.screen_height);
            self.append_fmt(format_args!("SCREEN: {} X {}", width, height));
            let (total, usable, _) = memory_summary();
            self.append_fmt(format_args!("TOTAL RAM: {} MB", total));
            self.append_fmt(format_args!("FREE RAM: {} MB", usable));
            self.append_uptime();
        } else if command.eq_ignore_ascii_case(b"mem") {
            let (total, usable, reserved) = memory_summary();
            let used = total.saturating_sub(usable);
            let percent = if total == 0 { 0 } else { used * 100 / total };
            let filled = ((percent as usize) * 10 / 100).min(10);
            self.append_fmt(format_args!("TOTAL MEMORY: {} MB", total));
            self.append_fmt(format_args!("USABLE MEMORY: {} MB", usable));
            self.append_fmt(format_args!("RESERVED MEMORY: {} MB", reserved));
            let mut bar = [b'-'; 10];
            for slot in bar.iter_mut().take(filled) {
                *slot = b'#';
            }
            let mut line = [0u8; 18];
            line[..1].copy_from_slice(b"[");
            line[1..11].copy_from_slice(&bar);
            line[11] = b']';
            let suffix = format_usage(percent);
            let suffix_len = suffix.len().min(6);
            line[12..12 + suffix_len].copy_from_slice(&suffix[..suffix_len]);
            self.append(&line[..12 + suffix_len]);
        } else if command.eq_ignore_ascii_case(b"uptime") {
            self.append_uptime();
        } else if command.eq_ignore_ascii_case(b"cpu") {
            self.append(b"ARCHITECTURE: ARM64");
            let el = read_current_el();
            let midr = read_midr();
            self.append_fmt(format_args!("EXCEPTION LEVEL: EL{}", el));
            self.append_fmt(format_args!("MIDR_EL1: 0X{:08X}", midr));
        } else if command.eq_ignore_ascii_case(b"system") {
            self.append(b"ARYOS ARM64 - DEVELOPMENT BUILD");
            self.append_firmware();
            let (width, height) = (self.screen_width, self.screen_height);
            self.append_fmt(format_args!("DISPLAY {} X {} PIXELS", width, height));
            let (total, usable, _) = memory_summary();
            self.append_fmt(format_args!("RAM {} MB TOTAL / {} MB FREE", total, usable));
            self.append_uptime();
            self.append_link_status();
            self.append(b"HOST WI-FI DETAILS ARE NOT EXPOSED.");
        } else if command.eq_ignore_ascii_case(b"wifi") {
            self.append_link_status();
            self.append(b"HOST WI-FI NAME AND SIGNAL ARE NOT EXPOSED.");
        } else if command.eq_ignore_ascii_case(b"ping") {
            self.append(b"USAGE: PING <HOSTNAME OR IPV4 ADDRESS>");
        } else if starts_with_ignore_ascii_case(command, b"ping ") {
            let host = match core::str::from_utf8(command[5..].trim_ascii()) {
                Ok(host) if !host.is_empty() => host,
                _ => "",
            };
            if host.is_empty() {
                self.append(b"USAGE: PING <HOSTNAME OR IPV4 ADDRESS>");
            } else {
                match ping_host(host) {
                    Ok((address, elapsed_ms)) => self.append_fmt(format_args!(
                        "REPLY FROM {}: ICMP ECHO, TIME={} MS",
                        address, elapsed_ms
                    )),
                    Err(PingError::LinkDown) => {
                        self.append(b"PING ERROR: NETWORK LINK IS DOWN");
                    }
                    Err(PingError::DhcpTimeout) => {
                        self.append(b"PING ERROR: DHCP CONFIGURATION TIMED OUT");
                    }
                    Err(PingError::NoDns) => {
                        self.append(b"PING ERROR: DHCP PROVIDED NO DNS SERVER");
                    }
                    Err(PingError::DnsFailure) => {
                        self.append(b"PING ERROR: DNS LOOKUP FAILED");
                    }
                    Err(PingError::Timeout) => {
                        self.append(b"PING: REQUEST TIMED OUT");
                    }
                    Err(PingError::NetworkIo) => {
                        self.append(b"PING ERROR: UEFI NETWORK I/O FAILED");
                    }
                    Err(PingError::Firmware(status)) => {
                        self.append_fmt(format_args!("PING ERROR: UEFI STATUS {:?}", status));
                    }
                    Err(PingError::UnsupportedInterface) => {
                        self.append(b"PING ERROR: UNSUPPORTED ETHERNET INTERFACE");
                    }
                    Err(PingError::E1000(e1000::InitError::NotFound)) => {
                        self.append(b"PING ERROR: UEFI NETWORK AND E1000 NOT FOUND");
                    }
                    Err(PingError::E1000(e1000::InitError::Firmware(stage, status))) => {
                        self.append_fmt(format_args!(
                            "E1000 ERROR: {}: UEFI STATUS {:?}",
                            stage, status
                        ));
                    }
                    Err(PingError::E1000(e1000::InitError::UnsupportedBar)) => {
                        self.append(b"E1000 ERROR: UNSUPPORTED PCI BAR");
                    }
                    Err(PingError::E1000(e1000::InitError::InvalidMac)) => {
                        self.append(b"E1000 ERROR: INVALID HARDWARE ADDRESS");
                    }
                    Err(PingError::E1000(e1000::InitError::DmaUnavailable)) => {
                        self.append(b"E1000 ERROR: DMA MEMORY UNAVAILABLE");
                    }
                }
            }
        } else if command.eq_ignore_ascii_case(b"history") {
            let count = self.history_count.min(10);
            let start = if self.history_count >= 10 {
                self.history_count % 10
            } else {
                0
            };
            for index in 0..count {
                let slot = (start + index) % 10;
                let length = self.history_lengths[slot];
                let mut line = [0u8; MAX_LINE_LENGTH];
                line[..length].copy_from_slice(&self.history[slot][..length]);
                self.append(&line[..length]);
            }
        } else if command.eq_ignore_ascii_case(b"color") {
            self.accent = (self.accent + 1) % 4;
        } else if starts_with_ignore_ascii_case(command, b"calc ") {
            let expression = &command[5..];
            match calculate(expression) {
                Some(result) => self.append_fmt(format_args!("RESULT: {}", result)),
                None => self.append(b"CALC: INVALID EXPRESSION"),
            }
        } else if command.eq_ignore_ascii_case(b"banner") {
            self.append(b" AAAAA  RRRR  Y   Y  OOO   SSS");
            self.append(b"AA   AA RR RR  Y Y  OO OO SS");
            self.append(b"AAAAAAA RRRR    Y   OO OO  SS");
            self.append(b"AA   AA RR RR   Y   OO OO   SS");
            self.append(b"AA   AA RR  RR  Y    OOO   SSS");
        } else if command.eq_ignore_ascii_case(b"whoami") {
            self.append(b"root@aryos");
        } else if command.eq_ignore_ascii_case(b"neofetch") {
            self.append(b"ARYOS      A SMALL ARM64 OS");
            self.append(b"OS         ARYOS ARM64");
            self.append(b"ARCH       ARM64");
            let (total, usable, _) = memory_summary();
            self.append_fmt(format_args!(
                "RAM        {} MB TOTAL / {} MB FREE",
                total, usable
            ));
            self.append_uptime();
            let width = self.screen_width;
            let height = self.screen_height;
            self.append_fmt(format_args!("RESOLUTION {} X {}", width, height));
        } else if command.eq_ignore_ascii_case(b"reboot") {
            self.append(b"REBOOTING...");
            uefi::runtime::reset(uefi::runtime::ResetType::COLD, Status::SUCCESS, None);
        } else if command.eq_ignore_ascii_case(b"shutdown") {
            self.append(b"SHUTTING DOWN...");
            uefi::runtime::reset(uefi::runtime::ResetType::SHUTDOWN, Status::SUCCESS, None);
        } else if command.eq_ignore_ascii_case(b"exit") {
            self.append(b"Are you sure? (y/n)");
            self.exit_pending = true;
        } else if command.eq_ignore_ascii_case(b"crash") {
            self.append(b"KERNEL PANIC DEMO");
            self.append(b"ARYOS HAS STOPPED.");
        } else if starts_with_ignore_ascii_case(command, b"echo ") {
            self.append(&command[5..]);
        } else {
            let mut unknown = [0u8; MAX_LINE_LENGTH];
            let prefix = b"UNKNOWN COMMAND \"";
            unknown[..prefix.len()].copy_from_slice(prefix);
            let name_len = input_length.min(MAX_LINE_LENGTH - prefix.len() - 23);
            unknown[prefix.len()..prefix.len() + name_len].copy_from_slice(&input[..name_len]);
            let suffix = b"\". TYPE HELP.";
            let pos = prefix.len() + name_len;
            unknown[pos..pos + suffix.len()].copy_from_slice(suffix);
            self.append(&unknown[..pos + suffix.len()]);
        }

        self.input = [0; MAX_LINE_LENGTH];
        self.input_length = 0;
    }

    fn append_fmt(&mut self, args: fmt::Arguments<'_>) {
        let mut line = TextLine::new();
        let _ = line.write_fmt(args);
        self.append(&line.bytes[..line.length]);
    }

    fn append_link_status(&mut self) {
        match virtual_network_link() {
            VirtualLink::Up => self.append(b"VIRTUAL NETWORK LINK: UP"),
            VirtualLink::Down => self.append(b"VIRTUAL NETWORK LINK: DOWN"),
            VirtualLink::Unknown => self.append(b"VIRTUAL NETWORK LINK: UNKNOWN"),
            VirtualLink::Missing => self.append(b"UEFI NETWORK STATUS UNAVAILABLE"),
        }
    }

    fn append_uptime(&mut self) {
        let seconds = uptime_seconds(self.uptime_start);
        self.append_fmt(format_args!(
            "UPTIME: {:02}:{:02}:{:02}",
            seconds / 3600,
            (seconds / 60) % 60,
            seconds % 60,
        ));
    }

    fn append_firmware(&mut self) {
        let vendor = system::firmware_vendor();
        let mut line = [0u8; MAX_LINE_LENGTH];
        let prefix = b"VENDOR: ";
        line[..prefix.len()].copy_from_slice(prefix);
        let mut length = prefix.len();
        let mut unreadable = false;
        for c16 in vendor.iter() {
            let character = char::from(*c16);
            if character == '\0' {
                break;
            }
            if character.is_ascii() {
                line[length] = character as u8;
                length += 1;
                if length == MAX_LINE_LENGTH {
                    break;
                }
            } else {
                unreadable = true;
            }
        }
        if unreadable || length == prefix.len() {
            self.append(b"VENDOR: UNKNOWN");
        } else {
            self.append(&line[..length]);
        }
        self.append_fmt(format_args!(
            "FIRMWARE REVISION: 0X{:08X}",
            system::firmware_revision()
        ));
        self.append_fmt(format_args!("UEFI SPEC: {}", system::uefi_revision()));
    }

    fn history_add(&mut self, input: &[u8], length: usize) {
        let length = length.min(MAX_LINE_LENGTH);
        let slot = self.history_count % 10;
        self.history[slot][..length].copy_from_slice(&input[..length]);
        self.history_lengths[slot] = length;
        self.history_count = self.history_count.saturating_add(1).min(100000);
        self.history_index = None;
    }

    fn accent_color(&self) -> Color {
        match self.accent {
            1 => GREEN,
            2 => ORANGE,
            3 => PINK,
            _ => CYAN,
        }
    }

    fn recall_history(&mut self, up: bool) {
        if self.history_count == 0 {
            return;
        }
        let count = self.history_count.min(10);
        let newest_slot = (self.history_count - 1) % 10;
        let index = match self.history_index {
            Some(index) if up => index.saturating_sub(1),
            Some(index) => (index + 1).min(count - 1),
            None => newest_slot,
        };
        self.history_index = Some(index);
        let offset = if count == 10 {
            (newest_slot + 10 - index) % 10
        } else {
            index
        };
        let slot = if count == 10 { offset } else { index };
        let length = self.history_lengths[slot];
        self.input[..length].copy_from_slice(&self.history[slot][..length]);
        self.input[length..].fill(0);
        self.input_length = length;
    }
}

fn read_counter() -> u64 {
    let ticks: u64;
    unsafe {
        core::arch::asm!("mrs {}, cntvct_el0", out(reg) ticks);
    }
    ticks
}

fn read_counter_frequency() -> u64 {
    let frequency: u64;
    unsafe {
        core::arch::asm!("mrs {}, cntfrq_el0", out(reg) frequency);
    }
    if frequency == 0 { 1 } else { frequency }
}

fn uptime_seconds(start: u64) -> u64 {
    read_counter().saturating_sub(start) / read_counter_frequency()
}

fn read_current_el() -> u64 {
    let current_el: u64;
    unsafe {
        core::arch::asm!("mrs {}, CurrentEL", out(reg) current_el);
    }
    (current_el >> 2) & 3
}

fn read_midr() -> u64 {
    let midr: u64;
    unsafe {
        core::arch::asm!("mrs {}, MIDR_EL1", out(reg) midr);
    }
    midr
}

fn memory_summary() -> (u64, u64, u64) {
    match boot::memory_map(uefi::boot::MemoryType::LOADER_DATA) {
        Ok(map) => {
            let mut total = 0u64;
            let mut usable = 0u64;
            let mut reserved = 0u64;
            use uefi::mem::memory_map::{MemoryMap, MemoryType};
            for descriptor in map.entries() {
                let pages = descriptor.page_count;
                let bytes = pages.saturating_mul(4096);
                total = total.saturating_add(bytes);
                match descriptor.ty {
                    MemoryType::CONVENTIONAL | MemoryType::PERSISTENT_MEMORY => {
                        usable = usable.saturating_add(bytes);
                    }
                    MemoryType::RESERVED
                    | MemoryType::LOADER_CODE
                    | MemoryType::LOADER_DATA
                    | MemoryType::BOOT_SERVICES_CODE
                    | MemoryType::BOOT_SERVICES_DATA
                    | MemoryType::RUNTIME_SERVICES_CODE
                    | MemoryType::RUNTIME_SERVICES_DATA
                    | MemoryType::ACPI_RECLAIM
                    | MemoryType::ACPI_NON_VOLATILE
                    | MemoryType::MMIO
                    | MemoryType::MMIO_PORT_SPACE
                    | MemoryType::PAL_CODE => {
                        reserved = reserved.saturating_add(bytes);
                    }
                    _ => reserved = reserved.saturating_add(bytes),
                }
            }
            (
                total / 1024 / 1024,
                usable / 1024 / 1024,
                reserved / 1024 / 1024,
            )
        }
        Err(_) => (0, 0, 0),
    }
}

fn format_usage(percent: u64) -> [u8; 6] {
    let mut text = [b' '; 6];
    let digits = percent;
    if digits >= 100 {
        text[..4].copy_from_slice(b"100%");
    } else if digits >= 10 {
        text[0] = b'0' + (digits / 10) as u8;
        text[1] = b'0' + (digits % 10) as u8;
        text[2] = b'%';
    } else {
        text[0] = b' ';
        text[1] = b'0' + digits as u8;
        text[2] = b'%';
    }
    text
}

fn calculate(expression: &[u8]) -> Option<i64> {
    let mut operator = None;
    let mut operator_index = 0;
    for (index, byte) in expression.iter().enumerate() {
        if *byte == b'+' || *byte == b'-' || *byte == b'*' || *byte == b'/' {
            operator = Some(*byte);
            operator_index = index;
            break;
        }
    }
    let operator = operator?;
    let left = core::str::from_utf8(&expression[..operator_index])
        .ok()?
        .trim()
        .parse::<i64>()
        .ok()?;
    let right = core::str::from_utf8(&expression[operator_index + 1..])
        .ok()?
        .trim()
        .parse::<i64>()
        .ok()?;
    match operator {
        b'+' => left.checked_add(right),
        b'-' => left.checked_sub(right),
        b'*' => left.checked_mul(right),
        b'/' if right != 0 => left.checked_div(right),
        _ => None,
    }
}

fn virtual_network_link() -> VirtualLink {
    let Ok(handles) = boot::locate_handle_buffer(boot::SearchType::from_proto::<SimpleNetwork>())
    else {
        return VirtualLink::Missing;
    };

    let mut found_interface = false;
    for handle in handles.iter().copied() {
        let params = boot::OpenProtocolParams {
            handle,
            agent: boot::image_handle(),
            controller: None,
        };
        // The interface is read-only here and the scoped protocol is closed at
        // the end of the iteration.
        let Ok(network) = (unsafe {
            boot::open_protocol::<SimpleNetwork>(params, boot::OpenProtocolAttributes::GetProtocol)
        }) else {
            continue;
        };
        found_interface = true;
        let mode = network.mode();
        if !bool::from(mode.media_present_supported) {
            continue;
        }
        return if bool::from(mode.media_present) {
            VirtualLink::Up
        } else {
            VirtualLink::Down
        };
    }

    if found_interface {
        VirtualLink::Unknown
    } else {
        VirtualLink::Missing
    }
}

enum PingError {
    LinkDown,
    DhcpTimeout,
    NoDns,
    DnsFailure,
    Timeout,
    NetworkIo,
    Firmware(Status),
    UnsupportedInterface,
    E1000(e1000::InitError),
}

const MAX_ETHERNET_FRAME: usize = 2048;
const MAX_IN_FLIGHT_TX: usize = 4;

#[repr(align(16))]
struct EthernetFrame([u8; MAX_ETHERNET_FRAME]);

// SNP transmits asynchronously, so submitted frames stay allocated until reclaimed.
struct SnpDevice<'a> {
    network: &'a SimpleNetwork,
    capabilities: DeviceCapabilities,
    io_error: Cell<bool>,
    tx_buffers: [EthernetFrame; MAX_IN_FLIGHT_TX],
    tx_pending: [Cell<bool>; MAX_IN_FLIGHT_TX],
    next_tx: usize,
}

impl<'a> SnpDevice<'a> {
    fn new(network: &'a SimpleNetwork) -> Result<Self, PingError> {
        let mode = network.mode();
        if mode.hw_address_size != 6 {
            return Err(PingError::UnsupportedInterface);
        }

        let mut capabilities = DeviceCapabilities::default();
        capabilities.medium = Medium::Ethernet;
        capabilities.max_transmission_unit = match mode.max_packet_size {
            0 => 1500,
            size => (size as usize).min(MAX_ETHERNET_FRAME - 14),
        };

        Ok(Self {
            network,
            capabilities,
            io_error: Cell::new(false),
            tx_buffers: [const { EthernetFrame([0; MAX_ETHERNET_FRAME]) }; MAX_IN_FLIGHT_TX],
            tx_pending: [const { Cell::new(false) }; MAX_IN_FLIGHT_TX],
            next_tx: 0,
        })
    }

    fn has_io_error(&self) -> bool {
        self.io_error.get()
    }

    fn reclaim_transmit_buffers(&self) {
        loop {
            let recycled = match self.network.get_recycled_transmit_buffer_status() {
                Ok(Some(recycled)) => recycled,
                Ok(None) => break,
                Err(_) => {
                    self.io_error.set(true);
                    break;
                }
            };
            let recycled = recycled.as_ptr();
            if let Some(index) = self
                .tx_buffers
                .iter()
                .position(|buffer| buffer.0.as_ptr() == recycled)
            {
                self.tx_pending[index].set(false);
            } else {
                self.io_error.set(true);
                break;
            }
        }
    }

    fn next_transmit_token(&mut self) -> Option<SnpTxToken<'_>> {
        self.reclaim_transmit_buffers();
        let Some(index) = (0..MAX_IN_FLIGHT_TX)
            .map(|offset| (self.next_tx + offset) % MAX_IN_FLIGHT_TX)
            .find(|index| !self.tx_pending[*index].get())
        else {
            self.io_error.set(true);
            return None;
        };
        self.next_tx = (index + 1) % MAX_IN_FLIGHT_TX;
        Some(SnpTxToken {
            network: self.network,
            io_error: &self.io_error,
            buffer: &mut self.tx_buffers[index],
            pending: &self.tx_pending[index],
        })
    }
}

struct SnpRxToken {
    frame: [u8; MAX_ETHERNET_FRAME],
    length: usize,
}

impl RxToken for SnpRxToken {
    fn consume<R, F>(self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        f(&self.frame[..self.length])
    }
}

struct SnpTxToken<'a> {
    network: &'a SimpleNetwork,
    io_error: &'a Cell<bool>,
    buffer: &'a mut EthernetFrame,
    pending: &'a Cell<bool>,
}

impl TxToken for SnpTxToken<'_> {
    fn consume<R, F>(self, length: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        if length > self.buffer.0.len() {
            self.io_error.set(true);
            self.pending.set(false);
            return f(&mut self.buffer.0);
        }
        let result = f(&mut self.buffer.0[..length]);
        match self
            .network
            .transmit(0, &self.buffer.0[..length], None, None, None)
        {
            Ok(()) => self.pending.set(true),
            Err(_) => {
                self.io_error.set(true);
                self.pending.set(false);
            }
        }
        result
    }
}

impl Device for SnpDevice<'_> {
    type RxToken<'a>
        = SnpRxToken
    where
        Self: 'a;
    type TxToken<'a>
        = SnpTxToken<'a>
    where
        Self: 'a;

    fn receive(&mut self, _timestamp: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        let mut frame = [0; MAX_ETHERNET_FRAME];
        match self.network.receive(&mut frame, None, None, None, None) {
            Ok(length) if length <= frame.len() => {
                Some((SnpRxToken { frame, length }, self.next_transmit_token()?))
            }
            Ok(_) => {
                self.io_error.set(true);
                None
            }
            Err(error) if error.status() == Status::NOT_READY => None,
            Err(_) => {
                self.io_error.set(true);
                None
            }
        }
    }

    fn transmit(&mut self, _timestamp: Instant) -> Option<Self::TxToken<'_>> {
        self.next_transmit_token()
    }

    fn capabilities(&self) -> DeviceCapabilities {
        self.capabilities.clone()
    }
}

fn ping_host(host: &str) -> Result<(Ipv4Address, u64), PingError> {
    let controllers = boot::locate_handle_buffer(boot::SearchType::from_proto::<DevicePath>())
        .map_err(|error| PingError::Firmware(error.status()))?;
    for controller in controllers.iter().copied() {
        if let Err(error) = boot::connect_controller(controller, None, None, true) {
            let status = error.status();
            if status != Status::NOT_FOUND && status != Status::UNSUPPORTED {
                return Err(PingError::Firmware(status));
            }
        }
    }

    let handles = match boot::locate_handle_buffer(boot::SearchType::from_proto::<SimpleNetwork>())
    {
        Ok(handles) => handles,
        Err(_) => return ping_via_e1000(host),
    };

    for handle in handles.iter().copied() {
        let params = boot::OpenProtocolParams {
            handle,
            agent: boot::image_handle(),
            controller: None,
        };
        let Ok(network) = (unsafe {
            boot::open_protocol::<SimpleNetwork>(params, boot::OpenProtocolAttributes::GetProtocol)
        }) else {
            continue;
        };

        let mode = network.mode();
        if bool::from(mode.media_present_supported) && !bool::from(mode.media_present) {
            continue;
        }

        return ping_on_interface(&network, host);
    }

    ping_via_e1000(host)
}

fn ping_via_e1000(host: &str) -> Result<(Ipv4Address, u64), PingError> {
    let handles =
        boot::locate_handle_buffer(boot::SearchType::from_proto::<e1000::PciRootBridgeIo>())
            .map_err(|_| PingError::E1000(e1000::InitError::NotFound))?;

    for handle in handles.iter().copied() {
        let params = boot::OpenProtocolParams {
            handle,
            agent: boot::image_handle(),
            controller: None,
        };
        let Ok(mut root) = (unsafe {
            boot::open_protocol::<e1000::PciRootBridgeIo>(
                params,
                boot::OpenProtocolAttributes::GetProtocol,
            )
        }) else {
            continue;
        };

        match e1000::E1000Device::new(&mut root) {
            Ok(mut device) => {
                let hardware_address = device.hardware_address();
                let result = run_ping_stack(&mut device, hardware_address, host);
                if device.has_io_error() {
                    return Err(PingError::NetworkIo);
                }
                return result;
            }
            Err(e1000::InitError::NotFound) => {}
            Err(error) => return Err(PingError::E1000(error)),
        }
    }

    Err(PingError::E1000(e1000::InitError::NotFound))
}

fn ping_on_interface(network: &SimpleNetwork, host: &str) -> Result<(Ipv4Address, u64), PingError> {
    let initial_state = network.mode().state;
    let started_here = initial_state == uefi::proto::network::snp::NetworkState::STOPPED;
    if started_here {
        network
            .start()
            .map_err(|error| PingError::Firmware(error.status()))?;
    }

    let initialized_here =
        network.mode().state != uefi::proto::network::snp::NetworkState::INITIALIZED;
    if initialized_here {
        if let Err(error) = network.initialize(0, 0) {
            if started_here {
                let _ = network.stop();
            }
            return Err(PingError::Firmware(error.status()));
        }
    }

    let result = (|| {
        let mode = network.mode();
        if bool::from(mode.media_present_supported) && !bool::from(mode.media_present) {
            return Err(PingError::LinkDown);
        }

        use uefi::proto::network::snp::ReceiveFlags;
        let supported_filters =
            ReceiveFlags::from_bits_truncate(network.mode().receive_filter_mask);
        network
            .receive_filters(
                supported_filters
                    & (ReceiveFlags::UNICAST | ReceiveFlags::BROADCAST | ReceiveFlags::MULTICAST),
                ReceiveFlags::empty(),
                false,
                None,
            )
            .map_err(|error| PingError::Firmware(error.status()))?;

        let mut device = SnpDevice::new(network)?;
        let mode = network.mode();
        let hardware_address = EthernetAddress([
            mode.current_address.0[0],
            mode.current_address.0[1],
            mode.current_address.0[2],
            mode.current_address.0[3],
            mode.current_address.0[4],
            mode.current_address.0[5],
        ]);
        let result = run_ping_stack(&mut device, hardware_address, host);
        if device.has_io_error() {
            return Err(PingError::NetworkIo);
        }
        result
    })();

    let shutdown_result = if initialized_here {
        network
            .shutdown()
            .map_err(|error| PingError::Firmware(error.status()))
    } else {
        Ok(())
    };
    let stop_result = if started_here {
        network
            .stop()
            .map_err(|error| PingError::Firmware(error.status()))
    } else {
        Ok(())
    };

    match (result, shutdown_result, stop_result) {
        (Err(error), _, _) => Err(error),
        (Ok(_), Err(error), _) | (Ok(_), Ok(()), Err(error)) => Err(error),
        (Ok(value), Ok(()), Ok(())) => Ok(value),
    }
}

fn network_instant(start: u64) -> Instant {
    let elapsed =
        read_counter().saturating_sub(start).saturating_mul(1000) / read_counter_frequency();
    Instant::from_millis(elapsed.min(i64::MAX as u64) as i64)
}

fn parse_ipv4(host: &str) -> Option<Ipv4Address> {
    let mut octets = [0u8; 4];
    let mut parts = host.split('.');
    for octet in &mut octets {
        *octet = parts.next()?.parse().ok()?;
    }
    if parts.next().is_some() {
        return None;
    }
    Some(Ipv4Address::new(octets[0], octets[1], octets[2], octets[3]))
}

fn run_ping_stack<D: Device>(
    device: &mut D,
    ethernet_address: EthernetAddress,
    host: &str,
) -> Result<(Ipv4Address, u64), PingError> {
    let hardware_address = HardwareAddress::Ethernet(ethernet_address);
    let capabilities = device.capabilities();
    let mut config = NetConfig::new(hardware_address);
    config.random_seed = read_counter();

    let mut socket_storage = [
        SocketStorage::EMPTY,
        SocketStorage::EMPTY,
        SocketStorage::EMPTY,
    ];
    let mut sockets = SocketSet::new(&mut socket_storage[..]);
    let dhcp_handle = sockets.add(dhcpv4::Socket::new());
    let started_at = read_counter();
    let mut interface = Interface::new(config, device, network_instant(started_at));
    let dhcp_started = uptime_milliseconds(started_at);
    let dns_server = loop {
        let now = network_instant(started_at);
        interface.poll(now, device, &mut sockets);
        if let Some(event) = sockets.get_mut::<dhcpv4::Socket>(dhcp_handle).poll() {
            if let dhcpv4::Event::Configured(config) = event {
                interface.update_ip_addrs(|addresses| {
                    addresses.clear();
                    let _ = addresses.push(IpCidr::Ipv4(config.address));
                });
                if let Some(router) = config.router {
                    interface
                        .routes_mut()
                        .add_default_ipv4_route(router)
                        .map_err(|_| PingError::NetworkIo)?;
                }
                break config.dns_servers.first().copied();
            }
        }
        if uptime_milliseconds(started_at).saturating_sub(dhcp_started) >= 10_000 {
            return Err(PingError::DhcpTimeout);
        }
    };

    let mut query_storage = [None];
    let address = if let Some(address) = parse_ipv4(host) {
        address
    } else {
        let dns_server = dns_server.ok_or(PingError::NoDns)?;
        let servers = [
            IpAddress::Ipv4(dns_server),
            IpAddress::Ipv4(Ipv4Address::new(8, 8, 8, 8)),
        ];
        let dns_socket = dns::Socket::new(&servers, &mut query_storage[..]);
        let dns_handle = sockets.add(dns_socket);
        let query = sockets
            .get_mut::<dns::Socket>(dns_handle)
            .start_query(interface.context(), host, DnsQueryType::A)
            .map_err(|_| PingError::DnsFailure)?;
        let dns_started = uptime_milliseconds(started_at);
        loop {
            let now = network_instant(started_at);
            interface.poll(now, device, &mut sockets);
            match sockets
                .get_mut::<dns::Socket>(dns_handle)
                .get_query_result(query)
            {
                Ok(addresses) => {
                    let IpAddress::Ipv4(address) =
                        addresses.first().copied().ok_or(PingError::DnsFailure)?;
                    break address;
                }
                Err(dns::GetQueryResultError::Pending) => {}
                Err(_) => return Err(PingError::DnsFailure),
            }
            if uptime_milliseconds(started_at).saturating_sub(dns_started) >= 22_000 {
                return Err(PingError::DnsFailure);
            }
        }
    };

    let mut icmp_rx_metadata = [icmp::PacketMetadata::EMPTY];
    let mut icmp_tx_metadata = [icmp::PacketMetadata::EMPTY];
    let mut icmp_rx_bytes = [0; 128];
    let mut icmp_tx_bytes = [0; 128];
    let icmp_socket = icmp::Socket::new(
        icmp::PacketBuffer::new(&mut icmp_rx_metadata[..], &mut icmp_rx_bytes[..]),
        icmp::PacketBuffer::new(&mut icmp_tx_metadata[..], &mut icmp_tx_bytes[..]),
    );
    let icmp_handle = sockets.add(icmp_socket);
    let socket = sockets.get_mut::<icmp::Socket>(icmp_handle);
    let identifier = 0x4152;
    socket
        .bind(icmp::Endpoint::Ident(identifier))
        .map_err(|_| PingError::NetworkIo)?;
    let payload = [b'A'; 24];
    let sequence = 1;
    let repr = Icmpv4Repr::EchoRequest {
        ident: identifier,
        seq_no: sequence,
        data: &payload,
    };
    let packet = socket
        .send(repr.buffer_len(), IpAddress::Ipv4(address))
        .map_err(|_| PingError::NetworkIo)?;
    repr.emit(
        &mut Icmpv4Packet::new_unchecked(packet),
        &capabilities.checksum,
    );
    let sent_at = uptime_milliseconds(started_at);

    loop {
        let now = network_instant(started_at);
        interface.poll(now, device, &mut sockets);
        let socket = sockets.get_mut::<icmp::Socket>(icmp_handle);
        if socket.can_recv() {
            let (packet, _) = socket.recv().map_err(|_| PingError::NetworkIo)?;
            if let Ok(packet) = Icmpv4Packet::new_checked(packet) {
                if let Ok(Icmpv4Repr::EchoReply { ident, seq_no, .. }) =
                    Icmpv4Repr::parse(&packet, &capabilities.checksum)
                {
                    if ident == identifier && seq_no == sequence {
                        return Ok((address, uptime_milliseconds(started_at) - sent_at));
                    }
                }
            }
        }
        if uptime_milliseconds(started_at).saturating_sub(sent_at) >= 4_000 {
            return Err(PingError::Timeout);
        }
    }
}

fn uptime_milliseconds(start: u64) -> u64 {
    read_counter().saturating_sub(start).saturating_mul(1000) / read_counter_frequency()
}

struct TextLine {
    bytes: [u8; MAX_LINE_LENGTH],
    length: usize,
}

impl TextLine {
    const fn new() -> Self {
        Self {
            bytes: [0; MAX_LINE_LENGTH],
            length: 0,
        }
    }
}

impl Write for TextLine {
    fn write_str(&mut self, text: &str) -> fmt::Result {
        for byte in text.bytes() {
            if self.length == self.bytes.len() {
                break;
            }
            self.bytes[self.length] = byte;
            self.length += 1;
        }
        Ok(())
    }
}

#[entry]
fn main() -> Status {
    uefi::helpers::init().unwrap();

    let gop_handle = match boot::get_handle_for_protocol::<GraphicsOutput>() {
        Ok(handle) => handle,
        Err(_) => return text_fallback(),
    };
    let gop = match boot::open_protocol_exclusive::<GraphicsOutput>(gop_handle) {
        Ok(gop) => gop,
        Err(_) => return text_fallback(),
    };

    if gop.current_mode_info().pixel_format() == PixelFormat::BltOnly {
        return text_fallback();
    }
    drop(gop);

    graphical_console(gop_handle)
}

fn graphical_console(gop_handle: Handle) -> Status {
    system::with_stdin(|stdin| {
        let Ok(mut gop) = boot::open_protocol_exclusive::<GraphicsOutput>(gop_handle) else {
            return text_fallback();
        };
        let mode = gop.current_mode_info();
        let (width, height) = mode.resolution();
        let stride = mode.stride();
        let format = mode.pixel_format();
        let bitmask = mode.pixel_bitmask();
        let mut framebuffer = gop.frame_buffer();
        let mut terminal = Terminal::new(width, height);
        terminal.uptime_start = read_counter();
        let mut blink_ticks = 0usize;
        draw_ui(
            &mut framebuffer,
            width,
            height,
            stride,
            format,
            bitmask,
            &terminal,
        );

        loop {
            match stdin.read_key() {
                Ok(Some(Key::Printable(key))) => {
                    let character = char::from(key);
                    blink_ticks = 0;
                    terminal.cursor_visible = true;
                    if terminal.exit_pending {
                        match character {
                            'y' | 'Y' | 'n' | 'N' => {
                                terminal.input = [0; MAX_LINE_LENGTH];
                                terminal.input[0] = character as u8;
                                terminal.input_length = 1;
                                terminal.submit();
                                draw_terminal(
                                    &mut framebuffer,
                                    width,
                                    height,
                                    stride,
                                    format,
                                    bitmask,
                                    &terminal,
                                );
                            }
                            '\r' | '\n' => {}
                            '\u{8}' | '\u{7f}' => {}
                            _ => {}
                        }
                    } else {
                        match character {
                            '\r' | '\n' => {
                                terminal.submit();
                                draw_ui(
                                    &mut framebuffer,
                                    width,
                                    height,
                                    stride,
                                    format,
                                    bitmask,
                                    &terminal,
                                );
                            }
                            '\u{8}' | '\u{7f}' => {
                                if terminal.input_length > 0 {
                                    terminal.input_length -= 1;
                                    terminal.input[terminal.input_length] = 0;
                                    draw_prompt(
                                        &mut framebuffer,
                                        width,
                                        height,
                                        stride,
                                        format,
                                        bitmask,
                                        &terminal,
                                    );
                                }
                            }
                            character if character.is_ascii_graphic() || character == ' ' => {
                                if terminal.input_length < MAX_LINE_LENGTH {
                                    terminal.input[terminal.input_length] = character as u8;
                                    terminal.input_length += 1;
                                    draw_prompt(
                                        &mut framebuffer,
                                        width,
                                        height,
                                        stride,
                                        format,
                                        bitmask,
                                        &terminal,
                                    );
                                }
                            }
                            _ => {}
                        }
                    }
                }
                Ok(Some(Key::Special(scan))) => {
                    if scan == uefi::proto::console::text::ScanCode::UP {
                        terminal.recall_history(true);
                        draw_prompt(
                            &mut framebuffer,
                            width,
                            height,
                            stride,
                            format,
                            bitmask,
                            &terminal,
                        );
                    } else if scan == uefi::proto::console::text::ScanCode::DOWN {
                        terminal.recall_history(false);
                        draw_prompt(
                            &mut framebuffer,
                            width,
                            height,
                            stride,
                            format,
                            bitmask,
                            &terminal,
                        );
                    }
                }
                Ok(None) => {}
                Err(_) => {}
            }

            boot::stall(50_000);
            blink_ticks += 1;
            if blink_ticks >= 10 {
                blink_ticks = 0;
                terminal.cursor_visible = !terminal.cursor_visible;
                draw_cursor(
                    &mut framebuffer,
                    width,
                    height,
                    stride,
                    format,
                    bitmask,
                    &terminal,
                );
            }
        }
    })
}

fn text_fallback() -> Status {
    system::with_stdout(|stdout| {
        let _ = stdout.output_string(cstr16!("ARYOS ARM64\r\n").as_ref());
        let _ = stdout
            .output_string(cstr16!("Graphics unavailable; text console active.\r\n").as_ref());
        let _ = stdout.output_string(cstr16!("Type help and press Enter.\r\n\r\nARYOS> ").as_ref());
    });

    system::with_stdin(|stdin| {
        loop {
            match stdin.read_key() {
                Ok(Some(Key::Printable(key))) => {
                    let character = char::from(key);
                    match character {
                        '\r' | '\n' => {
                            let _ = system::with_stdout(|stdout| {
                                stdout.output_string(cstr16!("\r\nARYOS> ").as_ref())
                            });
                        }
                        '\u{8}' | '\u{7f}' => {
                            let _ = system::with_stdout(|stdout| {
                                stdout.output_string(cstr16!("\u{8} \u{8}").as_ref())
                            });
                        }
                        character if character.is_ascii_graphic() || character == ' ' => {
                            let _ = system::with_stdout(|stdout| {
                                core::fmt::Write::write_char(stdout, character)
                            });
                        }
                        _ => {}
                    }
                }
                Ok(Some(Key::Special(_))) | Ok(None) => core::hint::spin_loop(),
                Err(_) => core::hint::spin_loop(),
            }
        }
    })
}

fn draw_ui(
    framebuffer: &mut uefi::proto::console::gop::FrameBuffer<'_>,
    width: usize,
    height: usize,
    stride: usize,
    format: PixelFormat,
    bitmask: Option<uefi::proto::console::gop::PixelBitmask>,
    terminal: &Terminal,
) {
    let accent = terminal.accent_color();
    fill_rect(
        framebuffer,
        width,
        height,
        stride,
        format,
        bitmask,
        0,
        0,
        width,
        height,
        BACKGROUND,
    );

    // Soft color bands give the flat background some depth without slowing boot.
    fill_rect(
        framebuffer,
        width,
        height,
        stride,
        format,
        bitmask,
        0,
        0,
        width,
        76,
        HEADER,
    );
    fill_rect(
        framebuffer,
        width,
        height,
        stride,
        format,
        bitmask,
        0,
        74,
        width,
        2,
        BORDER,
    );
    fill_rect(
        framebuffer,
        width,
        height,
        stride,
        format,
        bitmask,
        0,
        76,
        8,
        height.saturating_sub(76),
        accent,
    );

    draw_text(
        framebuffer,
        width,
        height,
        stride,
        format,
        bitmask,
        b"ARYOS",
        42,
        24,
        3,
        accent,
    );
    draw_text(
        framebuffer,
        width,
        height,
        stride,
        format,
        bitmask,
        b"ARM64",
        158,
        31,
        2,
        MUTED,
    );

    let online_x = width.saturating_sub(156);
    fill_circle(
        framebuffer,
        width,
        height,
        stride,
        format,
        bitmask,
        online_x,
        37,
        6,
        GREEN,
    );
    draw_text(
        framebuffer,
        width,
        height,
        stride,
        format,
        bitmask,
        b"SYSTEM READY",
        online_x + 16,
        30,
        2,
        WHITE,
    );

    draw_text(
        framebuffer,
        width,
        height,
        stride,
        format,
        bitmask,
        b"WELCOME TO YOUR WORKSPACE",
        54,
        111,
        2,
        accent,
    );

    let panel_x = 48;
    let panel_y = height * 32 / 100;
    let panel_width = width.saturating_sub(96);
    let panel_height = height.saturating_sub(panel_y + 52);
    fill_rect(
        framebuffer,
        width,
        height,
        stride,
        format,
        bitmask,
        panel_x,
        panel_y,
        panel_width,
        panel_height,
        PANEL,
    );
    fill_rect(
        framebuffer,
        width,
        height,
        stride,
        format,
        bitmask,
        panel_x,
        panel_y,
        panel_width,
        2,
        BORDER,
    );
    fill_rect(
        framebuffer,
        width,
        height,
        stride,
        format,
        bitmask,
        panel_x,
        panel_y + 48,
        panel_width,
        1,
        BORDER,
    );

    draw_text(
        framebuffer,
        width,
        height,
        stride,
        format,
        bitmask,
        b"COMMAND CENTER",
        panel_x + 24,
        panel_y + 17,
        2,
        WHITE,
    );
    fill_circle(
        framebuffer,
        width,
        height,
        stride,
        format,
        bitmask,
        panel_x + panel_width - 94,
        panel_y + 24,
        4,
        RED,
    );
    fill_circle(
        framebuffer,
        width,
        height,
        stride,
        format,
        bitmask,
        panel_x + panel_width - 78,
        panel_y + 24,
        4,
        ORANGE,
    );
    fill_circle(
        framebuffer,
        width,
        height,
        stride,
        format,
        bitmask,
        panel_x + panel_width - 62,
        panel_y + 24,
        4,
        GREEN,
    );

    let content_x = panel_x + 25;
    let content_y = panel_y + 64;
    let content_width = panel_width.saturating_sub(50);
    let content_height = panel_height.saturating_sub(110);
    fill_rect(
        framebuffer,
        width,
        height,
        stride,
        format,
        bitmask,
        content_x,
        content_y,
        content_width,
        content_height,
        PANEL_INNER,
    );
    draw_text(
        framebuffer,
        width,
        height,
        stride,
        format,
        bitmask,
        b"WELCOME TO ARYOS ARM64.",
        content_x + 18,
        content_y + 20,
        FONT_SCALE,
        WHITE,
    );
    draw_text(
        framebuffer,
        width,
        height,
        stride,
        format,
        bitmask,
        b"TYPE HELP TO SEE AVAILABLE COMMANDS.",
        content_x + 18,
        content_y + 48,
        FONT_SCALE,
        MUTED,
    );
    draw_terminal(
        framebuffer,
        width,
        height,
        stride,
        format,
        bitmask,
        terminal,
    );

    let footer_y = height.saturating_sub(31);
    draw_text(
        framebuffer,
        width,
        height,
        stride,
        format,
        bitmask,
        b"ENTER RUNS COMMAND    BACKSPACE EDITS",
        54,
        footer_y,
        1,
        MUTED,
    );
    draw_text(
        framebuffer,
        width,
        height,
        stride,
        format,
        bitmask,
        b"ARYOS DEVELOPMENT BUILD",
        width.saturating_sub(222),
        footer_y,
        1,
        MUTED,
    );
}

fn draw_terminal(
    framebuffer: &mut uefi::proto::console::gop::FrameBuffer<'_>,
    width: usize,
    height: usize,
    stride: usize,
    format: PixelFormat,
    bitmask: Option<uefi::proto::console::gop::PixelBitmask>,
    terminal: &Terminal,
) {
    let panel_x = 48;
    let panel_y = height * 32 / 100;
    let panel_width = width.saturating_sub(96);
    let panel_height = height.saturating_sub(panel_y + 52);
    let content_x = panel_x + 25;
    let content_y = panel_y + 64;
    let content_width = panel_width.saturating_sub(50);
    let content_height = panel_height.saturating_sub(110);

    fill_rect(
        framebuffer,
        width,
        height,
        stride,
        format,
        bitmask,
        content_x,
        content_y,
        content_width,
        content_height,
        PANEL_INNER,
    );
    draw_text(
        framebuffer,
        width,
        height,
        stride,
        format,
        bitmask,
        b"WELCOME TO ARYOS ARM64.",
        content_x + 18,
        content_y + 20,
        FONT_SCALE,
        WHITE,
    );
    draw_text(
        framebuffer,
        width,
        height,
        stride,
        format,
        bitmask,
        b"TYPE HELP TO SEE AVAILABLE COMMANDS.",
        content_x + 18,
        content_y + 48,
        FONT_SCALE,
        MUTED,
    );

    let log_y = content_y + 90;
    let text_x = content_x + 18;
    let max_visible = content_height.saturating_sub(42) / LINE_HEIGHT;
    let start = terminal
        .line_count
        .saturating_sub(max_visible.saturating_sub(1));
    for (visible_row, row) in (start..terminal.line_count).enumerate() {
        let y = log_y + visible_row * LINE_HEIGHT;
        draw_text(
            framebuffer,
            width,
            height,
            stride,
            format,
            bitmask,
            &terminal.lines[row][..terminal.lengths[row]],
            text_x,
            y,
            FONT_SCALE,
            WHITE,
        );
    }

    let prompt_y = log_y + terminal.line_count.saturating_sub(start) * LINE_HEIGHT;
    draw_text(
        framebuffer,
        width,
        height,
        stride,
        format,
        bitmask,
        b"ARYOS> ",
        text_x,
        prompt_y,
        FONT_SCALE,
        CYAN,
    );
    draw_text(
        framebuffer,
        width,
        height,
        stride,
        format,
        bitmask,
        &terminal.input[..terminal.input_length],
        text_x + 7 * 6 * FONT_SCALE,
        prompt_y,
        FONT_SCALE,
        WHITE,
    );
    draw_cursor_at(
        framebuffer,
        width,
        height,
        stride,
        format,
        bitmask,
        terminal,
        text_x,
        prompt_y,
    );
}

fn draw_cursor(
    framebuffer: &mut uefi::proto::console::gop::FrameBuffer<'_>,
    width: usize,
    height: usize,
    stride: usize,
    format: PixelFormat,
    bitmask: Option<uefi::proto::console::gop::PixelBitmask>,
    terminal: &Terminal,
) {
    let panel_y = height * 32 / 100;
    let panel_width = width.saturating_sub(96);
    let panel_height = height.saturating_sub(panel_y + 52);
    let content_x = 48 + 25;
    let content_y = panel_y + 64;
    let content_height = panel_height.saturating_sub(110);
    let max_visible = content_height.saturating_sub(42) / LINE_HEIGHT;
    let start = terminal
        .line_count
        .saturating_sub(max_visible.saturating_sub(1));
    let log_y = content_y + 90;
    let prompt_y = log_y + terminal.line_count.saturating_sub(start) * LINE_HEIGHT;
    let text_x = content_x + 18;
    let _ = panel_width;
    draw_cursor_at(
        framebuffer,
        width,
        height,
        stride,
        format,
        bitmask,
        terminal,
        text_x,
        prompt_y,
    );
}

fn draw_cursor_at(
    framebuffer: &mut uefi::proto::console::gop::FrameBuffer<'_>,
    width: usize,
    height: usize,
    stride: usize,
    format: PixelFormat,
    bitmask: Option<uefi::proto::console::gop::PixelBitmask>,
    terminal: &Terminal,
    text_x: usize,
    prompt_y: usize,
) {
    let cursor_x = text_x + (7 + terminal.input_length) * 6 * FONT_SCALE;
    let color = if terminal.cursor_visible {
        CYAN
    } else {
        PANEL_INNER
    };
    fill_rect(
        framebuffer,
        width,
        height,
        stride,
        format,
        bitmask,
        cursor_x,
        prompt_y + 2,
        2,
        13,
        color,
    );
}

fn draw_prompt(
    framebuffer: &mut uefi::proto::console::gop::FrameBuffer<'_>,
    width: usize,
    height: usize,
    stride: usize,
    format: PixelFormat,
    bitmask: Option<uefi::proto::console::gop::PixelBitmask>,
    terminal: &Terminal,
) {
    draw_terminal(
        framebuffer,
        width,
        height,
        stride,
        format,
        bitmask,
        terminal,
    );
}

fn fill_rect(
    framebuffer: &mut uefi::proto::console::gop::FrameBuffer<'_>,
    width: usize,
    height: usize,
    stride: usize,
    format: PixelFormat,
    bitmask: Option<uefi::proto::console::gop::PixelBitmask>,
    x: usize,
    y: usize,
    rect_width: usize,
    rect_height: usize,
    color: Color,
) {
    let end_x = x.saturating_add(rect_width).min(width);
    let end_y = y.saturating_add(rect_height).min(height);
    for py in y.min(height)..end_y {
        for px in x.min(width)..end_x {
            put_pixel(framebuffer, stride, format, bitmask, px, py, color);
        }
    }
}

fn fill_circle(
    framebuffer: &mut uefi::proto::console::gop::FrameBuffer<'_>,
    width: usize,
    height: usize,
    stride: usize,
    format: PixelFormat,
    bitmask: Option<uefi::proto::console::gop::PixelBitmask>,
    center_x: usize,
    center_y: usize,
    radius: usize,
    color: Color,
) {
    let radius_squared = radius * radius;
    for dy in 0..=radius * 2 {
        for dx in 0..=radius * 2 {
            let ox = dx as isize - radius as isize;
            let oy = dy as isize - radius as isize;
            if (ox * ox + oy * oy) as usize <= radius_squared {
                let x = center_x as isize + ox;
                let y = center_y as isize + oy;
                if x >= 0 && y >= 0 {
                    put_pixel(
                        framebuffer,
                        stride,
                        format,
                        bitmask,
                        x as usize,
                        y as usize,
                        color,
                    );
                }
            }
        }
    }
    let _ = (width, height);
}

fn draw_text(
    framebuffer: &mut uefi::proto::console::gop::FrameBuffer<'_>,
    width: usize,
    height: usize,
    stride: usize,
    format: PixelFormat,
    bitmask: Option<uefi::proto::console::gop::PixelBitmask>,
    text: &[u8],
    x: usize,
    y: usize,
    scale: usize,
    color: Color,
) {
    for (index, byte) in text.iter().copied().enumerate() {
        draw_char(
            framebuffer,
            width,
            height,
            stride,
            format,
            bitmask,
            byte,
            x + index * 6 * scale,
            y,
            scale,
            color,
        );
    }
}

fn draw_char(
    framebuffer: &mut uefi::proto::console::gop::FrameBuffer<'_>,
    width: usize,
    height: usize,
    stride: usize,
    format: PixelFormat,
    bitmask: Option<uefi::proto::console::gop::PixelBitmask>,
    character: u8,
    x: usize,
    y: usize,
    scale: usize,
    color: Color,
) {
    for (row, bits) in glyph(character).iter().copied().enumerate() {
        for column in 0..5 {
            if bits & (1 << (4 - column)) != 0 {
                fill_rect(
                    framebuffer,
                    width,
                    height,
                    stride,
                    format,
                    bitmask,
                    x + column * scale,
                    y + row * scale,
                    scale,
                    scale,
                    color,
                );
            }
        }
    }
}

fn put_pixel(
    framebuffer: &mut uefi::proto::console::gop::FrameBuffer<'_>,
    stride: usize,
    format: PixelFormat,
    bitmask: Option<uefi::proto::console::gop::PixelBitmask>,
    x: usize,
    y: usize,
    Color(red, green, blue): Color,
) {
    let offset = (y * stride + x).saturating_mul(4);
    if offset.saturating_add(4) > framebuffer.size() {
        return;
    }

    let bytes = match format {
        PixelFormat::Rgb => [red, green, blue, 0],
        PixelFormat::Bgr => [blue, green, red, 0],
        PixelFormat::Bitmask => {
            let mask = bitmask.unwrap_or_default();
            let packed = masked_channel(red, mask.red)
                | masked_channel(green, mask.green)
                | masked_channel(blue, mask.blue);
            packed.to_ne_bytes()
        }
        PixelFormat::BltOnly => return,
    };

    unsafe { framebuffer.write_value(offset, bytes) };
}

fn masked_channel(channel: u8, mask: u32) -> u32 {
    if mask == 0 {
        return 0;
    }
    let shift = mask.trailing_zeros();
    let max = mask >> shift;
    ((u32::from(channel) * max + 127) / 255 << shift) & mask
}

fn starts_with_ignore_ascii_case(value: &[u8], prefix: &[u8]) -> bool {
    value.len() >= prefix.len() && value[..prefix.len()].eq_ignore_ascii_case(prefix)
}

fn glyph(character: u8) -> [u8; 7] {
    match character.to_ascii_uppercase() {
        b'A' => [14, 17, 17, 31, 17, 17, 17],
        b'B' => [30, 17, 17, 30, 17, 17, 30],
        b'C' => [14, 17, 16, 16, 16, 17, 14],
        b'D' => [30, 17, 17, 17, 17, 17, 30],
        b'E' => [31, 16, 16, 30, 16, 16, 31],
        b'F' => [31, 16, 16, 30, 16, 16, 16],
        b'G' => [14, 17, 16, 23, 17, 17, 15],
        b'H' => [17, 17, 17, 31, 17, 17, 17],
        b'I' => [14, 4, 4, 4, 4, 4, 14],
        b'J' => [7, 2, 2, 2, 18, 18, 12],
        b'K' => [17, 18, 20, 24, 20, 18, 17],
        b'L' => [16, 16, 16, 16, 16, 16, 31],
        b'M' => [17, 27, 21, 21, 17, 17, 17],
        b'N' => [17, 25, 25, 21, 19, 19, 17],
        b'O' => [14, 17, 17, 17, 17, 17, 14],
        b'P' => [30, 17, 17, 30, 16, 16, 16],
        b'Q' => [14, 17, 17, 17, 21, 18, 13],
        b'R' => [30, 17, 17, 30, 20, 18, 17],
        b'S' => [15, 16, 16, 14, 1, 1, 30],
        b'T' => [31, 4, 4, 4, 4, 4, 4],
        b'U' => [17, 17, 17, 17, 17, 17, 14],
        b'V' => [17, 17, 17, 17, 17, 10, 4],
        b'W' => [17, 17, 17, 21, 21, 21, 10],
        b'X' => [17, 17, 10, 4, 10, 17, 17],
        b'Y' => [17, 17, 10, 4, 4, 4, 4],
        b'Z' => [31, 1, 2, 4, 8, 16, 31],
        b'0' => [14, 17, 19, 21, 25, 17, 14],
        b'1' => [4, 12, 4, 4, 4, 4, 14],
        b'2' => [14, 17, 1, 2, 4, 8, 31],
        b'3' => [30, 1, 1, 14, 1, 1, 30],
        b'4' => [2, 6, 10, 18, 31, 2, 2],
        b'5' => [31, 16, 16, 30, 1, 1, 30],
        b'6' => [14, 16, 16, 30, 17, 17, 14],
        b'7' => [31, 1, 2, 4, 8, 8, 8],
        b'8' => [14, 17, 17, 14, 17, 17, 14],
        b'9' => [14, 17, 17, 15, 1, 1, 14],
        b'>' => [16, 8, 4, 2, 4, 8, 16],
        b'<' => [1, 2, 4, 8, 4, 2, 1],
        b':' => [0, 4, 4, 0, 4, 4, 0],
        b'-' => [0, 0, 0, 31, 0, 0, 0],
        b'_' => [0, 0, 0, 0, 0, 0, 31],
        b'.' => [0, 0, 0, 0, 0, 12, 12],
        b'!' => [4, 4, 4, 4, 4, 0, 4],
        b'/' => [1, 2, 2, 4, 8, 8, 16],
        b'?' => [14, 17, 1, 2, 4, 0, 4],
        b'=' => [0, 31, 0, 31, 0, 0, 0],
        b',' => [0, 0, 0, 0, 4, 4, 8],
        b'(' => [2, 4, 8, 8, 8, 4, 2],
        b')' => [8, 4, 2, 2, 2, 4, 8],
        b'\'' => [4, 4, 8, 0, 0, 0, 0],
        b' ' => [0; 7],
        _ => [0; 7],
    }
}

use core::ffi::c_void;
use core::ptr;
use core::sync::atomic::{Ordering, fence};

use smoltcp::phy::{Device, DeviceCapabilities, Medium, RxToken, TxToken};
use smoltcp::time::Instant;
use smoltcp::wire::EthernetAddress;
use uefi::boot::{self, AllocateType, MemoryType};
use uefi::proto::unsafe_protocol;
use uefi::{Error, Status, StatusExt};

const PCI_VENDOR_INTEL: u16 = 0x8086;
const PCI_DEVICE_82540EM: u16 = 0x100e;
const RX_COUNT: usize = 16;
const TX_COUNT: usize = 16;
const FRAME_SIZE: usize = 2048;
const PAGE_SIZE: usize = 4096;

const REG_CTRL: u32 = 0x0000;
const REG_IMC: u32 = 0x00d8;
const REG_RCTL: u32 = 0x0100;
const REG_TCTL: u32 = 0x0400;
const REG_TIPG: u32 = 0x0410;
const REG_RDBAL: u32 = 0x2800;
const REG_RDBAH: u32 = 0x2804;
const REG_RDLEN: u32 = 0x2808;
const REG_RDH: u32 = 0x2810;
const REG_RDT: u32 = 0x2818;
const REG_TDBAL: u32 = 0x3800;
const REG_TDBAH: u32 = 0x3804;
const REG_TDLEN: u32 = 0x3808;
const REG_TDH: u32 = 0x3810;
const REG_TDT: u32 = 0x3818;
const REG_RAL: u32 = 0x5400;
const REG_RAH: u32 = 0x5404;

const CTRL_RST: u32 = 1 << 26;
const CTRL_SLU: u32 = 1 << 6;
const RCTL_ENABLE: u32 = 1 << 1;
const RCTL_BROADCAST: u32 = 1 << 15;
const RCTL_STRIP_CRC: u32 = 1 << 26;
const TCTL_ENABLE: u32 = 1 << 1;
const TCTL_PAD_SHORT_PACKETS: u32 = 1 << 3;
const TX_CMD_EOP: u8 = 1;
const TX_CMD_IFCS: u8 = 1 << 1;
const TX_CMD_RS: u8 = 1 << 3;
const DESC_DONE: u8 = 1;
const DMA_COMMON_BUFFER: u32 = 2;

type ProtocolRead =
    unsafe extern "efiapi" fn(*mut RawPciRootBridgeIo, u32, u64, usize, *mut c_void) -> Status;
type ProtocolWrite =
    unsafe extern "efiapi" fn(*mut RawPciRootBridgeIo, u32, u64, usize, *const c_void) -> Status;

#[repr(C)]
struct PciRootBridgeIoAccess {
    read: ProtocolRead,
    write: ProtocolWrite,
}

#[repr(C)]
struct RawPciRootBridgeIo {
    parent_handle: *mut c_void,
    poll_mem: unsafe extern "efiapi" fn(*mut Self, u32, u64, u64, u64, u64, *mut u64) -> Status,
    poll_io: unsafe extern "efiapi" fn(*mut Self, u32, u64, u64, u64, u64, *mut u64) -> Status,
    mem: PciRootBridgeIoAccess,
    io: PciRootBridgeIoAccess,
    pci: PciRootBridgeIoAccess,
    copy_mem: unsafe extern "efiapi" fn(*mut Self, u32, u64, u64, usize) -> Status,
    map: unsafe extern "efiapi" fn(
        *const Self,
        u32,
        *const c_void,
        *mut usize,
        *mut u64,
        *mut *mut c_void,
    ) -> Status,
    unmap: unsafe extern "efiapi" fn(*const Self, *const c_void) -> Status,
    allocate_buffer:
        unsafe extern "efiapi" fn(*const Self, u32, u32, usize, *mut *const c_void, u64) -> Status,
    free_buffer: unsafe extern "efiapi" fn(*const Self, usize, *const c_void) -> Status,
    flush: unsafe extern "efiapi" fn(*mut Self) -> Status,
    get_attributes: unsafe extern "efiapi" fn(*const Self, *mut u64, *mut u64) -> Status,
    set_attributes: unsafe extern "efiapi" fn(*mut Self, u64, *mut u64, *mut u64) -> Status,
    configuration: unsafe extern "efiapi" fn(*const Self, *mut *const c_void) -> Status,
    segment_number: u32,
}

#[repr(transparent)]
#[unsafe_protocol("2f707ebb-4a1a-11d4-9a38-0090273fc14d")]
pub struct PciRootBridgeIo(RawPciRootBridgeIo);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InitError {
    NotFound,
    Firmware(&'static str, Status),
    UnsupportedBar,
    InvalidMac,
    DmaUnavailable,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Descriptor {
    address: u64,
    length: u16,
    checksum: u16,
    status: u8,
    errors: u8,
    special: u16,
}

struct DmaRegion {
    root: *mut PciRootBridgeIo,
    host: ptr::NonNull<u8>,
    device: u64,
    mapping: *mut c_void,
    pages: usize,
    allocated: bool,
}

impl DmaRegion {
    fn allocate(root: &mut PciRootBridgeIo, pages: usize) -> Result<Self, InitError> {
        let host = boot::allocate_pages(
            AllocateType::MaxAddress(u32::MAX as u64),
            MemoryType::LOADER_DATA,
            pages,
        )
        .map_err(|error| InitError::Firmware("DMA ALLOC", error.status()))?;

        let mut length = pages.saturating_mul(PAGE_SIZE);
        let mut device = 0;
        let mut mapping = ptr::null_mut();
        let status = unsafe {
            (root.0.map)(
                &root.0,
                DMA_COMMON_BUFFER,
                host.as_ptr().cast(),
                &mut length,
                &mut device,
                &mut mapping,
            )
        };
        if let Err(status) = status.to_result() {
            let _ = unsafe { boot::free_pages(host, pages) };
            return Err(InitError::Firmware("DMA MAP", status.status()));
        }

        if length < pages.saturating_mul(PAGE_SIZE) {
            unsafe {
                let _ = (root.0.unmap)(&root.0, mapping);
                let _ = boot::free_pages(host, pages);
            }
            return Err(InitError::DmaUnavailable);
        }

        unsafe { ptr::write_bytes(host.as_ptr(), 0, pages.saturating_mul(PAGE_SIZE)) };
        Ok(Self {
            root: root as *mut PciRootBridgeIo,
            host,
            device,
            mapping,
            pages,
            allocated: true,
        })
    }

    fn release(&mut self) -> Result<(), Error> {
        let root = unsafe { &mut *self.root };
        if !self.mapping.is_null() {
            let status = unsafe { (root.0.unmap)(&root.0, self.mapping) };
            status.to_result()?;
            self.mapping = ptr::null_mut();
        }
        if self.allocated {
            unsafe { boot::free_pages(self.host, self.pages) }?;
            self.allocated = false;
        }
        Ok(())
    }
}

impl Drop for DmaRegion {
    fn drop(&mut self) {
        let _ = self.release();
    }
}

pub struct E1000Device<'a> {
    root: &'a mut PciRootBridgeIo,
    mmio: u64,
    mac: EthernetAddress,
    rx_ring: DmaRegion,
    tx_ring: DmaRegion,
    rx_buffers: DmaRegion,
    tx_buffers: DmaRegion,
    rx_index: usize,
    tx_index: usize,
    capabilities: DeviceCapabilities,
    io_error: bool,
    initialized: bool,
}

impl<'a> E1000Device<'a> {
    pub fn new(root: &'a mut PciRootBridgeIo) -> Result<Self, InitError> {
        let (address, bar) = find_82540em(root)?;
        let command = pci_read_u16(root, address.with_register(0x04))?;
        pci_write_u16(root, address.with_register(0x04), command | 0x0006)?;
        let rx_ring = DmaRegion::allocate(root, 1)?;
        let tx_ring = DmaRegion::allocate(root, 1)?;
        let rx_buffers = DmaRegion::allocate(root, RX_COUNT * FRAME_SIZE / PAGE_SIZE)?;
        let tx_buffers = DmaRegion::allocate(root, TX_COUNT * FRAME_SIZE / PAGE_SIZE)?;

        let mut device = Self {
            root,
            mmio: bar,
            mac: EthernetAddress([0; 6]),
            rx_ring,
            tx_ring,
            rx_buffers,
            tx_buffers,
            rx_index: 0,
            tx_index: 0,
            capabilities: DeviceCapabilities::default(),
            io_error: false,
            initialized: false,
        };

        device.capabilities.medium = Medium::Ethernet;
        device.capabilities.max_transmission_unit = 1500;

        if let Err(error) = device.initialize() {
            device.shutdown();
            return Err(error);
        }
        device.initialized = true;
        Ok(device)
    }

    pub fn hardware_address(&self) -> EthernetAddress {
        self.mac
    }

    pub fn has_io_error(&self) -> bool {
        self.io_error
    }

    fn initialize(&mut self) -> Result<(), InitError> {
        self.write_reg(REG_IMC, u32::MAX)?;
        let control = self.read_reg(REG_CTRL)?;
        self.write_reg(REG_CTRL, control | CTRL_RST)?;
        let start = read_counter();
        while self.read_reg(REG_CTRL)? & CTRL_RST != 0 {
            if elapsed_millis(start) > 250 {
                return Err(InitError::Firmware("DEVICE RESET TIMEOUT", Status::TIMEOUT));
            }
        }

        self.write_reg(REG_IMC, u32::MAX)?;
        let control = self.read_reg(REG_CTRL)?;
        self.write_reg(REG_CTRL, control | CTRL_SLU)?;
        self.configure_rings()?;

        let low = self.read_reg(REG_RAL)?;
        let high = self.read_reg(REG_RAH)?;
        if high & (1 << 31) == 0 {
            return Err(InitError::InvalidMac);
        }
        self.mac = EthernetAddress([
            low as u8,
            (low >> 8) as u8,
            (low >> 16) as u8,
            (low >> 24) as u8,
            high as u8,
            (high >> 8) as u8,
        ]);
        if self.mac.0.iter().all(|byte| *byte == 0 || *byte == 0xff) {
            return Err(InitError::InvalidMac);
        }

        Ok(())
    }

    fn configure_rings(&mut self) -> Result<(), InitError> {
        let rx_descriptors = self.rx_ring.host.as_ptr().cast::<Descriptor>();
        let tx_descriptors = self.tx_ring.host.as_ptr().cast::<Descriptor>();
        for index in 0..RX_COUNT {
            let descriptor = unsafe { rx_descriptors.add(index) };
            unsafe {
                ptr::write_volatile(
                    descriptor,
                    Descriptor {
                        address: self.rx_buffers.device + (index * FRAME_SIZE) as u64,
                        length: 0,
                        checksum: 0,
                        status: 0,
                        errors: 0,
                        special: 0,
                    },
                )
            };
        }
        for index in 0..TX_COUNT {
            let descriptor = unsafe { tx_descriptors.add(index) };
            unsafe {
                ptr::write_volatile(
                    descriptor,
                    Descriptor {
                        address: self.tx_buffers.device + (index * FRAME_SIZE) as u64,
                        length: 0,
                        checksum: 0,
                        status: DESC_DONE,
                        errors: 0,
                        special: 0,
                    },
                )
            };
        }
        fence(Ordering::SeqCst);

        self.write_reg(REG_RDBAL, self.rx_ring.device as u32)?;
        self.write_reg(REG_RDBAH, (self.rx_ring.device >> 32) as u32)?;
        self.write_reg(REG_RDLEN, (RX_COUNT * size_of::<Descriptor>()) as u32)?;
        self.write_reg(REG_RDH, 0)?;
        self.write_reg(REG_RDT, (RX_COUNT - 1) as u32)?;
        self.write_reg(REG_TDBAL, self.tx_ring.device as u32)?;
        self.write_reg(REG_TDBAH, (self.tx_ring.device >> 32) as u32)?;
        self.write_reg(REG_TDLEN, (TX_COUNT * size_of::<Descriptor>()) as u32)?;
        self.write_reg(REG_TDH, 0)?;
        self.write_reg(REG_TDT, 0)?;
        self.write_reg(REG_TIPG, 0x0060_200a)?;
        self.write_reg(REG_RCTL, RCTL_ENABLE | RCTL_BROADCAST | RCTL_STRIP_CRC)?;
        self.write_reg(
            REG_TCTL,
            TCTL_ENABLE | TCTL_PAD_SHORT_PACKETS | (0x10 << 4) | (0x40 << 12),
        )?;
        Ok(())
    }

    fn transmit_frame(&mut self, frame: &[u8]) -> Result<(), ()> {
        if frame.len() > FRAME_SIZE {
            return Err(());
        }
        let index = self.tx_index;
        let descriptor = unsafe { self.tx_ring.host.as_ptr().cast::<Descriptor>().add(index) };
        let start = read_counter();
        loop {
            fence(Ordering::SeqCst);
            let status = unsafe { ptr::read_volatile(ptr::addr_of!((*descriptor).status)) };
            if status & DESC_DONE != 0 {
                break;
            }
            if elapsed_millis(start) > 1000 {
                return Err(());
            }
        }

        let buffer = unsafe { self.tx_buffers.host.as_ptr().add(index * FRAME_SIZE) };
        unsafe { ptr::copy_nonoverlapping(frame.as_ptr(), buffer, frame.len()) };
        unsafe {
            ptr::write_volatile(ptr::addr_of_mut!((*descriptor).length), frame.len() as u16);
            ptr::write_volatile(ptr::addr_of_mut!((*descriptor).checksum), 0);
            ptr::write_volatile(ptr::addr_of_mut!((*descriptor).errors), 0);
            ptr::write_volatile(ptr::addr_of_mut!((*descriptor).status), 0);
            ptr::write_volatile(ptr::addr_of_mut!((*descriptor).special), 0);
            ptr::write_volatile(
                ptr::addr_of_mut!((*descriptor).address),
                self.tx_buffers.device + (index * FRAME_SIZE) as u64,
            );
            ptr::write_volatile(ptr::addr_of_mut!((*descriptor).status), 0);
            let command = TX_CMD_EOP | TX_CMD_IFCS | TX_CMD_RS;
            ptr::write_volatile((descriptor.cast::<u8>().add(11)).cast::<u8>(), command);
        }
        fence(Ordering::SeqCst);
        self.tx_index = (index + 1) % TX_COUNT;
        if self.write_reg(REG_TDT, self.tx_index as u32).is_err() {
            return Err(());
        }
        Ok(())
    }

    fn read_reg(&mut self, offset: u32) -> Result<u32, InitError> {
        let mut value = 0;
        let status = unsafe {
            (self.root.0.mem.read)(
                &mut self.root.0,
                2,
                self.mmio + offset as u64,
                1,
                (&mut value as *mut u32).cast(),
            )
        };
        status
            .to_result()
            .map_err(|error| InitError::Firmware("MMIO READ", error.status()))?;
        Ok(value)
    }

    fn write_reg(&mut self, offset: u32, value: u32) -> Result<(), InitError> {
        let status = unsafe {
            (self.root.0.mem.write)(
                &mut self.root.0,
                2,
                self.mmio + offset as u64,
                1,
                (&value as *const u32).cast(),
            )
        };
        status
            .to_result()
            .map_err(|error| InitError::Firmware("MMIO WRITE", error.status()))
    }

    fn shutdown(&mut self) {
        let _ = self.write_reg(REG_RCTL, 0);
        let _ = self.write_reg(REG_TCTL, 0);
    }
}

impl Drop for E1000Device<'_> {
    fn drop(&mut self) {
        if self.initialized {
            self.shutdown();
        }
    }
}

pub struct E1000RxToken {
    frame: [u8; FRAME_SIZE],
    length: usize,
}

impl RxToken for E1000RxToken {
    fn consume<R, F>(self, f: F) -> R
    where
        F: FnOnce(&[u8]) -> R,
    {
        f(&self.frame[..self.length])
    }
}

pub struct E1000TxToken<'a, 'root> {
    device: &'a mut E1000Device<'root>,
}

impl TxToken for E1000TxToken<'_, '_> {
    fn consume<R, F>(self, length: usize, f: F) -> R
    where
        F: FnOnce(&mut [u8]) -> R,
    {
        let mut frame = [0; FRAME_SIZE];
        let length = length.min(FRAME_SIZE);
        let result = f(&mut frame[..length]);
        if self.device.transmit_frame(&frame[..length]).is_err() {
            self.device.io_error = true;
        }
        result
    }
}

impl<'root> Device for E1000Device<'root> {
    type RxToken<'b>
        = E1000RxToken
    where
        Self: 'b;
    type TxToken<'b>
        = E1000TxToken<'b, 'root>
    where
        Self: 'b;

    fn receive(&mut self, _timestamp: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        let index = self.rx_index;
        let descriptor = unsafe { self.rx_ring.host.as_ptr().cast::<Descriptor>().add(index) };
        fence(Ordering::SeqCst);
        let status = unsafe { ptr::read_volatile(ptr::addr_of!((*descriptor).status)) };
        if status & DESC_DONE == 0 {
            return None;
        }

        let length = unsafe { ptr::read_volatile(ptr::addr_of!((*descriptor).length)) as usize };
        let mut frame = [0; FRAME_SIZE];
        if length <= frame.len() {
            let source = unsafe { self.rx_buffers.host.as_ptr().add(index * FRAME_SIZE) };
            unsafe { ptr::copy_nonoverlapping(source, frame.as_mut_ptr(), length) };
        } else {
            self.io_error = true;
        }
        unsafe {
            ptr::write_volatile(ptr::addr_of_mut!((*descriptor).length), 0);
            ptr::write_volatile(ptr::addr_of_mut!((*descriptor).errors), 0);
            ptr::write_volatile(ptr::addr_of_mut!((*descriptor).status), 0);
        }
        fence(Ordering::SeqCst);
        if self.write_reg(REG_RDT, index as u32).is_err() {
            self.io_error = true;
        }
        self.rx_index = (index + 1) % RX_COUNT;
        if length > frame.len() {
            return None;
        }

        Some((
            E1000RxToken { frame, length },
            E1000TxToken { device: self },
        ))
    }

    fn transmit(&mut self, _timestamp: Instant) -> Option<Self::TxToken<'_>> {
        Some(E1000TxToken { device: self })
    }

    fn capabilities(&self) -> DeviceCapabilities {
        self.capabilities.clone()
    }
}

fn find_82540em(root: &mut PciRootBridgeIo) -> Result<(PciAddress, u64), InitError> {
    let bus = 0;
    for device in 0..32 {
        let address = PciAddress {
            bus,
            device,
            function: 0,
        };
        let identity = pci_read_u32(root, address.with_register(0))?;
        if identity as u16 != PCI_VENDOR_INTEL || (identity >> 16) as u16 != PCI_DEVICE_82540EM {
            continue;
        }

        let bar_low = pci_read_u32(root, address.with_register(0x10))?;
        if bar_low & 1 != 0 {
            return Err(InitError::UnsupportedBar);
        }
        let bar_type = (bar_low >> 1) & 3;
        let mut mmio = (bar_low & !0x0f) as u64;
        if bar_type == 2 {
            let bar_high = pci_read_u32(root, address.with_register(0x14))?;
            mmio |= (bar_high as u64) << 32;
        } else if bar_type > 1 {
            return Err(InitError::UnsupportedBar);
        }
        if mmio == 0 {
            return Err(InitError::UnsupportedBar);
        }
        return Ok((address, mmio));
    }
    Err(InitError::NotFound)
}

#[derive(Clone, Copy)]
struct PciAddress {
    bus: u8,
    device: u8,
    function: u8,
}

impl PciAddress {
    const fn with_register(self, register: u8) -> u64 {
        (self.bus as u64) << 24
            | (self.device as u64) << 16
            | (self.function as u64) << 8
            | register as u64
    }
}

fn pci_read_u16(root: &mut PciRootBridgeIo, address: u64) -> Result<u16, InitError> {
    let mut value = 0;
    let status =
        unsafe { (root.0.pci.read)(&mut root.0, 1, address, 1, (&mut value as *mut u16).cast()) };
    status
        .to_result()
        .map_err(|error| InitError::Firmware("PCI READ", error.status()))?;
    Ok(value)
}

fn pci_write_u16(root: &mut PciRootBridgeIo, address: u64, value: u16) -> Result<(), InitError> {
    let status =
        unsafe { (root.0.pci.write)(&mut root.0, 1, address, 1, (&value as *const u16).cast()) };
    status
        .to_result()
        .map_err(|error| InitError::Firmware("PCI WRITE", error.status()))
}

fn pci_read_u32(root: &mut PciRootBridgeIo, address: u64) -> Result<u32, InitError> {
    let mut value = u32::MAX;
    let status =
        unsafe { (root.0.pci.read)(&mut root.0, 2, address, 1, (&mut value as *mut u32).cast()) };
    status
        .to_result()
        .map_err(|error| InitError::Firmware("PCI READ", error.status()))?;
    Ok(value)
}

fn read_counter() -> u64 {
    let ticks: u64;
    unsafe {
        core::arch::asm!("mrs {}, cntvct_el0", out(reg) ticks);
    }
    ticks
}

fn elapsed_millis(start: u64) -> u64 {
    let frequency: u64;
    unsafe {
        core::arch::asm!("mrs {}, cntfrq_el0", out(reg) frequency);
    }
    read_counter().saturating_sub(start).saturating_mul(1000) / frequency.max(1)
}

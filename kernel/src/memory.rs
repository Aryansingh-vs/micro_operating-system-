use bootloader_api::info::{MemoryRegionKind, MemoryRegions};
use linked_list_allocator::LockedHeap;
use x86_64::{
    structures::paging::{
        Mapper, OffsetPageTable, Page, PageTable, PhysFrame, Size4KiB,
    },
    PhysAddr, VirtAddr,
};

pub const HEAP_START: usize = 0x_4444_4444_0000;
pub const HEAP_SIZE: usize = 1024 * 1024;

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

/// Frame allocator backed directly by the bootloader memory map.
pub struct BootInfoFrameAllocator {
    regions: &'static MemoryRegions,
    next: usize,
}

impl BootInfoFrameAllocator {
    pub fn init(regions: &'static MemoryRegions) -> Self {
        Self { regions, next: 0 }
    }

    fn usable_frames(&self) -> impl Iterator<Item = PhysFrame> {
        let regions = self.regions.iter().filter(|r| r.kind == MemoryRegionKind::Usable);
        let addr_ranges = regions.map(|r| r.start..r.end);
        addr_ranges
            .flat_map(|range| range.step_by(4096))
            .map(|addr| PhysFrame::containing_address(PhysAddr::new(addr)))
    }
}

impl Iterator for BootInfoFrameAllocator {
    type Item = PhysFrame;

    fn next(&mut self) -> Option<Self::Item> {
        let frame = self.usable_frames().nth(self.next);
        self.next += 1;
        frame
    }

}

unsafe impl x86_64::structures::paging::FrameAllocator<Size4KiB> for BootInfoFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        self.next()
    }
}

pub unsafe fn init_offset_page_table(physical_offset: u64) -> OffsetPageTable<'static> {
    unsafe {
        let phys_offset = VirtAddr::new(physical_offset);
        let level4_table = active_level4_table(phys_offset);
        OffsetPageTable::new(&mut *level4_table, phys_offset)
    }
}

unsafe fn active_level4_table(physical_offset: VirtAddr) -> &'static mut PageTable {
    use x86_64::registers::control::Cr3;
    let (level4_table_frame, _) = Cr3::read();
    let phys = level4_table_frame.start_address();
    let virt = physical_offset + phys.as_u64();
    unsafe { &mut *virt.as_mut_ptr::<PageTable>() }
}

pub fn init_heap(mapper: &mut OffsetPageTable, allocator: &mut BootInfoFrameAllocator) {
    use x86_64::structures::paging::PageTableFlags as Flags;
    let page_range = {
        let heap_start = VirtAddr::new(HEAP_START as u64);
        let heap_end = heap_start + HEAP_SIZE as u64 - 1u64;
        let start_page = Page::<Size4KiB>::containing_address(heap_start);
        let end_page = Page::<Size4KiB>::containing_address(heap_end);
        Page::range_inclusive(start_page, end_page)
    };

    for page in page_range {
        let frame = allocator.next().expect("out of physical memory");
        let flags = Flags::PRESENT | Flags::WRITABLE;
        unsafe {
            mapper.map_to(page, frame, flags, allocator)
                .expect("heap map failed")
                .flush();
        }
    }

    unsafe {
        ALLOCATOR.lock().init(HEAP_START as *mut u8, HEAP_SIZE);
    }
}

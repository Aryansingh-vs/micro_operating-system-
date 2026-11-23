use spin::Lazy;
use x86_64::{
    instructions::{
        segmentation::{Segment, CS, DS},
        tables::load_tss,
    },
    structures::{
        gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector},
        tss::TaskStateSegment,
    },
    VirtAddr,
};

pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;

struct Selectors {
    code: SegmentSelector,
    tss: SegmentSelector,
    data: SegmentSelector,
}

static TSS: Lazy<TaskStateSegment> = Lazy::new(|| {
    let mut tss = TaskStateSegment::new();
    tss.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = {
        const STACK_SIZE: usize = 64 * 1024;
        static mut STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];
        let stack_start = VirtAddr::from_ptr(core::ptr::addr_of!(STACK));
        stack_start + STACK_SIZE as u64
    };
    tss
});

static GDT: Lazy<(GlobalDescriptorTable, Selectors)> = Lazy::new(|| {
    let mut gdt = GlobalDescriptorTable::new();
    let code = gdt.append(Descriptor::kernel_code_segment());
    let data = gdt.append(Descriptor::kernel_data_segment());
    let tss = gdt.append(Descriptor::tss_segment(&TSS));
    (
        gdt,
        Selectors { code, tss, data },
    )
});

pub fn init() {
    GDT.0.load();
    unsafe {
        CS::set_reg(GDT.1.code);
        DS::set_reg(GDT.1.data);
        load_tss(GDT.1.tss);
    }
}

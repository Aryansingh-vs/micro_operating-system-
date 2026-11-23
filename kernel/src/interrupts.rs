use crate::{serial_println, task};
use spin::{Lazy, Mutex};
use x86_64::instructions::interrupts;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame};

pub const PIC_1_OFFSET: u8 = 32;
pub const PIC_2_OFFSET: u8 = 40;

pub static PICS: Mutex<Pics> = Mutex::new(Pics::new());

pub struct Pics;

impl Pics {
    pub const fn new() -> Self { Self }

    pub fn init(&mut self) {
        use x86_64::instructions::port::Port;
        let mut cmd1 = Port::<u8>::new(0x20);
        let mut data1 = Port::<u8>::new(0x21);
        let mut cmd2 = Port::<u8>::new(0xa0);
        let mut data2 = Port::<u8>::new(0xa1);
        unsafe {
            cmd1.write(0x11); cmd2.write(0x11);
            data1.write(PIC_1_OFFSET); data2.write(PIC_2_OFFSET);
            data1.write(4); data2.write(2);
            data1.write(1); data2.write(1);
            data1.write(0); data2.write(0);
        }
    }

    pub fn end_of_interrupt(&mut self, irq: u8) {
        use x86_64::instructions::port::Port;
        let mut cmd1 = Port::<u8>::new(0x20);
        let mut cmd2 = Port::<u8>::new(0xa0);
        unsafe {
            if irq >= PIC_2_OFFSET { cmd2.write(0x20); }
            cmd1.write(0x20);
        }
    }
}

static IDT: Lazy<InterruptDescriptorTable> = Lazy::new(|| {
    let mut idt = InterruptDescriptorTable::new();
    idt.breakpoint.set_handler_fn(breakpoint_handler);
    unsafe {
        idt.double_fault
            .set_handler_fn(double_fault_handler)
            .set_stack_index(crate::gdt::DOUBLE_FAULT_IST_INDEX);
    }
    idt[InterruptIndex::Timer as u8].set_handler_fn(timer_handler);
    idt[InterruptIndex::Keyboard as u8].set_handler_fn(keyboard_handler);
    idt
});

#[derive(Clone, Copy)]
#[repr(u8)]
pub enum InterruptIndex {
    Timer = PIC_1_OFFSET,
    Keyboard = PIC_1_OFFSET + 1,
}


pub fn init_idt() {
    IDT.load();
}

extern "x86-interrupt" fn breakpoint_handler(stack_frame: InterruptStackFrame) {
    serial_println!("EXCEPTION: breakpoint\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn double_fault_handler(
    stack_frame: InterruptStackFrame,
    error_code: u64,
) -> ! {
    panic!("double fault: {error_code:#x}\n{:#?}", stack_frame);
}

extern "x86-interrupt" fn timer_handler(_frame: InterruptStackFrame) {
    task::TICK_COUNT.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    PICS.lock().end_of_interrupt(InterruptIndex::Timer as u8);
}

extern "x86-interrupt" fn keyboard_handler(_frame: InterruptStackFrame) {
    use x86_64::instructions::port::Port;
    let mut port = Port::new(0x60);
    let scancode: u8 = unsafe { port.read() };
    task::push_scancode(scancode);
    PICS.lock().end_of_interrupt(InterruptIndex::Keyboard as u8);
}

pub fn enable() {
    PICS.lock().init();
    interrupts::enable();
}

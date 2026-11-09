#![no_std]
#![no_main]

use core::{
    alloc::{GlobalAlloc, Layout},
    cell::UnsafeCell,
    ptr,
    sync::atomic::{AtomicUsize, Ordering},
};

// This single-run example uses a bump heap. A real firmware supplies its allocator.
struct Heap {
    bytes: UnsafeCell<[u8; 128 * 1024]>,
    cursor: AtomicUsize,
}
unsafe impl Sync for Heap {}
unsafe impl GlobalAlloc for Heap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let base = self.bytes.get().cast::<u8>() as usize;
        let mut cursor = self.cursor.load(Ordering::Relaxed);
        loop {
            let aligned = (base + cursor + layout.align() - 1) & !(layout.align() - 1);
            let next = aligned - base + layout.size();
            if next > 128 * 1024 {
                return ptr::null_mut();
            }
            match self.cursor.compare_exchange_weak(
                cursor,
                next,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return aligned as *mut u8,
                Err(actual) => cursor = actual,
            }
        }
    }
    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}
#[global_allocator]
static HEAP: Heap = Heap {
    bytes: UnsafeCell::new([0; 128 * 1024]),
    cursor: AtomicUsize::new(0),
};

extern "C" {
    static _stack_top: u8;
    static _sidata: u8;
    static mut _sdata: u8;
    static mut _edata: u8;
    static mut _sbss: u8;
    static mut _ebss: u8;
}
#[repr(C)]
struct Vectors {
    stack: *const u8,
    reset: unsafe extern "C" fn() -> !,
}
unsafe impl Sync for Vectors {}
#[used]
#[link_section = ".vector_table"]
static VECTORS: Vectors = Vectors {
    stack: ptr::addr_of!(_stack_top),
    reset: _start,
};

#[no_mangle]
unsafe extern "C" fn _start() -> ! {
    let data = ptr::addr_of_mut!(_sdata);
    let data_len = ptr::addr_of!(_edata) as usize - data as usize;
    ptr::copy_nonoverlapping(ptr::addr_of!(_sidata), data, data_len);
    let bss = ptr::addr_of_mut!(_sbss);
    let bss_len = ptr::addr_of!(_ebss) as usize - bss as usize;
    ptr::write_bytes(bss, 0, bss_len);
    assert!(goose_alloc_agent_example::run_demo().is_ok());
    loop {
        core::hint::spin_loop();
    }
}

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    loop {
        core::hint::spin_loop();
    }
}

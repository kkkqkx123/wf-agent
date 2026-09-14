//! Minimal wf-agent wasm guest: no_std, bump allocator, static JSON.
//!
//! Build with:
//!
//! ```sh
//! rustup target add wasm32-unknown-unknown
//! cargo build --release --target wasm32-unknown-unknown
//! ```
//!
//! The output `target/wasm32-unknown-unknown/release/wasm_echo.wasm`
//! loads directly as a `Wasm` plugin (see `plugin.toml` next to it).

#![no_std]
#![no_main]

use core::panic::PanicInfo;
use core::ptr::addr_of_mut;

static mut HEAP: [u8; HEAP_CAP] = [0; HEAP_CAP];
static mut HEAP_USED: usize = 0;

const HEAP_CAP: usize = 65536;
const DECL: &[u8] = br#"{"tool_types":["echo"]}"#;
const RESULT: &[u8] = br#"{"result":{"echo":true}}"#;

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    core::arch::wasm32::unreachable()
}

fn trap() -> ! {
    core::arch::wasm32::unreachable()
}

/// Copy `data` into guest memory, returning packed `(ptr, len)`.
fn emit(data: &[u8]) -> u64 {
    unsafe {
        let used = addr_of_mut!(HEAP_USED).read();
        let end = used + data.len();
        if end > HEAP_CAP {
            trap();
        }
        let base = addr_of_mut!(HEAP).cast::<u8>();
        core::ptr::copy_nonoverlapping(data.as_ptr(), base.add(used), data.len());
        addr_of_mut!(HEAP_USED).write(end);
        (base as u64 + used as u64) | ((data.len() as u64) << 32)
    }
}

#[no_mangle]
pub extern "C" fn alloc(size: u32) -> u32 {
    unsafe {
        let used = addr_of_mut!(HEAP_USED).read();
        let end = used + size as usize;
        if end > HEAP_CAP {
            trap();
        }
        let base = addr_of_mut!(HEAP).cast::<u8>();
        addr_of_mut!(HEAP_USED).write(end);
        base as u32 + used as u32
    }
}

/// Bump allocation is never reclaimed; the host drops the whole store
/// after each call, which bounds the leak to one invocation.
#[no_mangle]
pub extern "C" fn dealloc(_ptr: u32, _len: u32) {}

/// Opt into host session-pool reuse: restore the bump allocator so the
/// next call starts from a clean heap. Returning 0 marks this session
/// reusable; any other value makes the host discard it.
#[no_mangle]
pub extern "C" fn wf_heap_reset() -> u32 {
    unsafe {
        addr_of_mut!(HEAP_USED).write(0);
    }
    0
}

#[no_mangle]
pub extern "C" fn wf_on_load(_ptr: u32, _len: u32) -> u32 {
    0
}

#[no_mangle]
pub extern "C" fn wf_on_activate(_ptr: u32, _len: u32) -> u32 {
    0
}

#[no_mangle]
pub extern "C" fn wf_register() -> u64 {
    emit(DECL)
}

#[no_mangle]
pub extern "C" fn wf_dispatch(
    _type_ptr: u32,
    _type_len: u32,
    _name_ptr: u32,
    _name_len: u32,
    _input_ptr: u32,
    _input_len: u32,
) -> u64 {
    emit(RESULT)
}

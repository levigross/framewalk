// rust-unsafe-crash.rs — Null pointer dereference through unsafe code.
//
// Showcases: Rust debug info handling for unsafe blocks.  A chain of
// safe Rust functions leads into an unsafe block that dereferences a
// null pointer.  The backtrace shows the boundary between safe and
// unsafe Rust.
//
// Compile: rustc -g -o /tmp/rust-unsafe-crash examples/programs/rust-unsafe-crash.rs
//
// Scheme session — inspect the crash:
//   (begin
//     (load-file "/tmp/rust-unsafe-crash")
//     (run)
//     (wait-for-stop)           ;; catches SIGSEGV
//     (backtrace)
//     (list-locals))

struct RawBuffer {
    ptr: *mut u8,
    len: usize,
}

impl RawBuffer {
    fn new_null(len: usize) -> Self {
        RawBuffer {
            ptr: std::ptr::null_mut(),
            len,
        }
    }

    /// Reads a byte from the buffer — crashes if ptr is null.
    unsafe fn read_at(&self, offset: usize) -> u8 {
        assert!(offset < self.len, "offset out of bounds");
        // This dereferences a null pointer — SIGSEGV
        *self.ptr.add(offset)
    }
}

fn process_buffer(buf: &RawBuffer) -> u8 {
    println!("reading from buffer at {:?} (len={})", buf.ptr, buf.len);
    unsafe { buf.read_at(0) }
}

fn run_processing(buffers: &[RawBuffer]) {
    for (i, buf) in buffers.iter().enumerate() {
        println!("processing buffer {}", i);
        let byte = process_buffer(buf);
        println!("buffer {}: first byte = 0x{:02x}", i, byte);
    }
}

fn main() {
    let buffers = vec![
        RawBuffer::new_null(64),  // null pointer — will crash
    ];

    println!("starting unsafe buffer processing");
    run_processing(&buffers);
    println!("done (should not reach here)");
}

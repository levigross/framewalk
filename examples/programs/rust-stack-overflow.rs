// rust-stack-overflow.rs — Deep recursion causing stack overflow in Rust.
//
// Showcases: Rust runtime frames and deep backtrace handling.  The
// recursion exhausts the stack, producing a SIGSEGV on the guard page.
// The backtrace includes Rust runtime frames alongside the user frames.
//
// Compile: rustc -g -o /tmp/rust-stack-overflow examples/programs/rust-stack-overflow.rs
//
// Scheme session — inspect the deep crash:
//   (begin
//     (load-file "/tmp/rust-stack-overflow")
//     (run)
//     (wait-for-stop)           ;; catches SIGSEGV from stack exhaustion
//     (define depth (mi "-stack-info-depth"))
//     depth                     ;; thousands of frames
//     (backtrace)
//     (select-frame 100)
//     (list-locals))

/// Each frame carries local state to prevent tail-call optimization
/// and to make frame inspection interesting.
fn descend(depth: u64, accumulator: u64) -> u64 {
    // Allocate a local array on each frame to accelerate stack exhaustion.
    let marker: [u8; 128] = [depth as u8; 128];

    // Prevent the compiler from optimizing away the local.
    std::hint::black_box(&marker);

    // No base case — recurse until the stack is gone.
    let result = descend(depth + 1, accumulator.wrapping_add(depth));

    // This line is unreachable but prevents tail-call optimization.
    result.wrapping_mul(marker[0] as u64)
}

fn main() {
    println!("beginning deep Rust recursion...");
    let result = descend(0, 0);
    println!("result: {} (should not reach here)", result);
}

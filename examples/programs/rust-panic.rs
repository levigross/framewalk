// rust-panic.rs — Panic through generics producing a complex backtrace.
//
// Showcases: Rust symbol demangling and long backtraces.  A chain of
// generic functions produces deeply nested monomorphized names in the
// backtrace.  The panic goes through core::panicking and std's runtime.
//
// Compile: rustc -g -o /tmp/rust-panic examples/programs/rust-panic.rs
//
// Scheme session — catch the panic:
//   (begin
//     (load-file "/tmp/rust-panic")
//     (run)
//     (wait-for-stop)           ;; catches SIGABRT from panic
//     (backtrace)               ;; long backtrace with monomorphized names
//     (list-locals))

use std::fmt::Debug;

fn validate<T: Debug + PartialOrd>(items: &[T], threshold: &T) {
    for (i, item) in items.iter().enumerate() {
        if item > threshold {
            panic!(
                "validation failed: item[{}] = {:?} exceeds threshold {:?}",
                i, item, threshold
            );
        }
    }
}

fn process_batch<T: Debug + PartialOrd + Copy>(data: &[T], limit: &T) {
    let filtered: Vec<&T> = data.iter().filter(|x| *x <= limit).collect();
    println!("filtered {} items down to {}", data.len(), filtered.len());
    validate(data, limit);
}

fn run_pipeline<T: Debug + PartialOrd + Copy>(batches: &[Vec<T>], limit: &T) {
    for (batch_idx, batch) in batches.iter().enumerate() {
        println!("processing batch {} ({} items)", batch_idx, batch.len());
        process_batch(batch, limit);
    }
}

fn main() {
    let batches: Vec<Vec<i64>> = vec![
        vec![1, 2, 3, 4, 5],
        vec![10, 20, 30, 40, 50],
        vec![100, 200, 300, 400, 500],  // This batch will panic
    ];

    println!("running pipeline with limit 250");
    run_pipeline(&batches, &250_i64);
    println!("done (should not reach here)");
}

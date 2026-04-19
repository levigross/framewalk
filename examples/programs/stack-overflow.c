/* stack-overflow.c — Deep recursion blowing the stack.
 *
 * Showcases: deep backtrace handling.  The program recurses until the
 * stack guard page is hit, producing a SIGSEGV with thousands of frames.
 * Tests framewalk's ability to handle massive backtraces, frame pagination,
 * and selection at arbitrary depth.
 *
 * Compile: gcc -g -O0 -o /tmp/stack-overflow examples/programs/stack-overflow.c
 *
 * Scheme session — inspect the deep crash:
 *   (begin
 *     (load-file "/tmp/stack-overflow")
 *     (run)
 *     (wait-for-stop)          ;; catches SIGSEGV from stack exhaustion
 *     (define depth (mi "-stack-info-depth"))
 *     depth                    ;; shows thousands of frames
 *     (select-frame 500)       ;; jump to frame 500
 *     (list-locals)            ;; inspect locals at that depth
 *     (backtrace))             ;; full backtrace (may be very long)
 */

#include <stdio.h>

/* Each frame allocates a modest local buffer to accelerate stack
 * exhaustion without needing millions of recursions. */
void descend(int depth) {
    char marker[256];
    snprintf(marker, sizeof(marker), "frame-%d", depth);

    /* No base case — recurse forever until the stack is gone. */
    descend(depth + 1);

    /* Prevent tail-call optimization (the printf is never reached,
     * but the compiler can't prove that). */
    printf("marker: %s\n", marker);
}

int main(void) {
    printf("beginning deep recursion...\n");
    descend(0);
    return 0;
}

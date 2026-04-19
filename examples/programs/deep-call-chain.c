/* deep-call-chain.c — Controlled deep recursion with inspectable state.
 *
 * Showcases: backtrace depth handling and frame navigation at scale.
 * Unlike stack-overflow (uncontrolled), this recurses to a known depth
 * with meaningful locals at every frame, then dereferences NULL.
 *
 * Compile: gcc -g -O0 -o /tmp/deep-call-chain examples/programs/deep-call-chain.c
 *
 * Scheme session — navigate the deep backtrace:
 *   (begin
 *     (load-file "/tmp/deep-call-chain")
 *     (run)
 *     (wait-for-stop)           ;; catches SIGSEGV at the bottom
 *     (define depth (mi "-stack-info-depth"))
 *     depth                     ;; should show ~122 frames
 *     (select-frame 0)
 *     (list-locals)             ;; depth=120, tag="frame-120"
 *     (select-frame 60)
 *     (list-locals)             ;; depth=60, tag="frame-060"
 *     (select-frame 119)
 *     (list-locals))            ;; depth=1, tag="frame-001"
 */

#include <stdio.h>
#include <string.h>

#define MAX_DEPTH 120

/* Each frame carries identifiable local state. */
void call_chain(int depth, int max_depth) {
    char tag[32];
    int local_value = depth * 7 + 3;  /* arbitrary but deterministic */

    snprintf(tag, sizeof(tag), "frame-%03d", depth);

    if (depth >= max_depth) {
        /* Crash at the bottom of the chain. */
        printf("reached depth %d — crashing\n", depth);
        int *null = NULL;
        *null = depth;
        return;
    }

    /* Recurse deeper. */
    call_chain(depth + 1, max_depth);

    /* Prevent tail-call optimization. */
    printf("unwinding %s (value=%d)\n", tag, local_value);
}

int main(void) {
    printf("building call chain %d frames deep...\n", MAX_DEPTH);
    call_chain(0, MAX_DEPTH);
    return 0;
}

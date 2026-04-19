/* function-pointer-crash.c — Crash through a dispatch table of function pointers.
 *
 * Showcases: backtrace through indirect calls.  A pipeline of
 * processing stages is driven through a function pointer table.
 * One entry is NULL, causing a crash when dispatched.  GDB should
 * still resolve function names for the non-NULL entries in the
 * backtrace.
 *
 * Compile: gcc -g -O0 -o /tmp/function-pointer-crash examples/programs/function-pointer-crash.c
 *
 * Scheme session — inspect the dispatch table:
 *   (begin
 *     (load-file "/tmp/function-pointer-crash")
 *     (run)
 *     (wait-for-stop)           ;; catches SIGSEGV from calling NULL
 *     (backtrace)               ;; shows the call chain through the dispatcher
 *     ;; Inspect the dispatch table to find the NULL entry
 *     (inspect "pipeline[0]")
 *     (inspect "pipeline[1]")
 *     (inspect "pipeline[2]")
 *     (inspect "pipeline[3]"))
 */

#include <stdio.h>
#include <string.h>

typedef int (*stage_fn)(int value, int *result);

int stage_double(int value, int *result) {
    printf("stage_double: %d -> %d\n", value, value * 2);
    *result = value * 2;
    return 0;
}

int stage_add_ten(int value, int *result) {
    printf("stage_add_ten: %d -> %d\n", value, value + 10);
    *result = value + 10;
    return 0;
}

int stage_square(int value, int *result) {
    printf("stage_square: %d -> %d\n", value, value * value);
    *result = value * value;
    return 0;
}

/* Run each stage in sequence, passing the output of one to the next. */
int run_pipeline(stage_fn *pipeline, int num_stages, int initial) {
    int value = initial;

    for (int i = 0; i < num_stages; i++) {
        int result = 0;
        printf("dispatching stage %d (fn=%p)\n", i, (void *)pipeline[i]);

        /* If pipeline[i] is NULL, this is a call to address 0 — SIGSEGV. */
        int rc = pipeline[i](value, &result);
        if (rc != 0) {
            printf("stage %d failed with rc=%d\n", i, rc);
            return -1;
        }
        value = result;
    }

    return value;
}

int main(void) {
    /* Pipeline with a NULL hole at index 2. */
    stage_fn pipeline[] = {
        stage_double,
        stage_add_ten,
        NULL,           /* Bug: missing stage. */
        stage_square,
    };
    int num_stages = sizeof(pipeline) / sizeof(pipeline[0]);

    printf("running %d-stage pipeline with initial value 5\n", num_stages);
    int result = run_pipeline(pipeline, num_stages, 5);
    printf("result: %d\n", result);

    return 0;
}

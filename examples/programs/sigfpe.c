/* sigfpe.c — Division by zero producing SIGFPE.
 *
 * Showcases: arithmetic exception debugging.  The fault location is
 * a single instruction (idiv on x86), making this the cleanest signal
 * to inspect.  Demonstrates disassembly and register inspection at the
 * faulting instruction.
 *
 * Compile: gcc -g -O0 -o /tmp/sigfpe examples/programs/sigfpe.c
 *
 * Scheme session — inspect the faulting instruction:
 *   (begin
 *     (load-file "/tmp/sigfpe")
 *     (run)
 *     (wait-for-stop)           ;; catches SIGFPE
 *     (backtrace)
 *     (inspect "divisor")       ;; shows 0
 *     (mi "-data-disassemble -s $pc -e \"$pc+16\" -- 0")
 *     (mi "-data-list-register-values x"))
 */

#include <stdio.h>

/* The zero comes through a function call so GCC won't warn
 * about a literal division by zero. */
int get_divisor(int index) {
    int divisors[] = {10, 5, 2, 0, 1};
    return divisors[index];
}

int compute(int value, int divisor) {
    printf("computing %d / %d\n", value, divisor);
    return value / divisor;  /* SIGFPE when divisor == 0 */
}

int process_batch(int *values, int count) {
    int total = 0;
    for (int i = 0; i < count; i++) {
        int d = get_divisor(i);
        total += compute(values[i], d);
    }
    return total;
}

int main(void) {
    int values[] = {100, 200, 300, 400, 500};
    int count = 5;

    printf("processing %d values...\n", count);
    int result = process_batch(values, count);
    printf("result: %d\n", result);

    return 0;
}

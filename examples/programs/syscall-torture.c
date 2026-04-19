/* syscall-torture.c — Syzkaller-inspired rapid fork+syscall harness.
 *
 * Showcases: debugging a fuzzer-like harness.  Inspired by google/syzkaller's
 * C reproducer structure: a main loop that forks child processes to run
 * syscall sequences, with signal handlers and timeouts.  Each child
 * eventually triggers a crash.
 *
 * Compile: gcc -g -O0 -o /tmp/syscall-torture examples/programs/syscall-torture.c
 *
 * Scheme session — break inside the execution loop:
 *   (begin
 *     (load-file "/tmp/syscall-torture")
 *     (set-breakpoint "execute_one")
 *     (run)
 *     (wait-for-stop)
 *     (inspect "iteration")
 *     (inspect "test_id")
 *     (list-locals)
 *     ;; Continue to the crash
 *     (mi "-break-delete")
 *     (cont)
 *     (wait-for-stop)           ;; catches SIGSEGV in child
 *     (backtrace))
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <signal.h>
#include <unistd.h>
#include <sys/mman.h>
#include <sys/wait.h>

/* Scratch region for test data (syzkaller uses 0x20000000). */
static void *scratch;
#define SCRATCH_SIZE (4096 * 4)

/* Non-failing wrapper: suppress signals from intentionally bad syscalls. */
#define NONFAILING(...)                                     \
    do {                                                    \
        struct sigaction _sa_old, _sa_new;                  \
        memset(&_sa_new, 0, sizeof(_sa_new));               \
        _sa_new.sa_handler = SIG_IGN;                       \
        sigaction(SIGSEGV, &_sa_new, &_sa_old);             \
        __VA_ARGS__;                                        \
        sigaction(SIGSEGV, &_sa_old, NULL);                 \
    } while (0)

/* Individual test cases — each exercises a different crash path. */
void test_null_deref(void) {
    int *p = NULL;
    *p = 42;
}

void test_stack_smash(void) {
    char buf[16];
    memset(buf, 'A', 256);  /* Overflow the stack buffer. */
}

void test_bad_mmap_access(void) {
    /* Write to a read-only mapping. */
    void *p = mmap(NULL, 4096, PROT_READ, MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (p != MAP_FAILED) {
        *(int *)p = 42;  /* SIGSEGV: write to read-only page. */
    }
}

void test_div_zero(void) {
    volatile int a = 100;
    volatile int b = 0;
    volatile int c = a / b;
    (void)c;
}

typedef void (*test_fn)(void);

static test_fn tests[] = {
    test_null_deref,
    test_stack_smash,
    test_bad_mmap_access,
    test_div_zero,
};
#define NUM_TESTS (sizeof(tests) / sizeof(tests[0]))

/* Execute one test case — called in a forked child. */
void execute_one(int test_id, int iteration) {
    printf("  child[%d]: executing test %d, iteration %d\n",
           getpid(), test_id, iteration);

    /* Some non-failing operations to warm up the scratch region. */
    NONFAILING(memset(scratch, 0xCC, SCRATCH_SIZE));
    NONFAILING(((char *)scratch)[SCRATCH_SIZE - 1] = 0);

    /* Run the actual test — this will crash. */
    tests[test_id % NUM_TESTS]();
}

int main(void) {
    /* Allocate scratch region. */
    scratch = mmap(NULL, SCRATCH_SIZE, PROT_READ | PROT_WRITE,
                   MAP_PRIVATE | MAP_ANONYMOUS, -1, 0);
    if (scratch == MAP_FAILED) {
        perror("mmap scratch");
        return 1;
    }

    printf("syzkaller-style harness: %d test cases\n", (int)NUM_TESTS);

    for (int iter = 0; iter < 4; iter++) {
        int test_id = iter % NUM_TESTS;

        printf("iteration %d: forking for test %d\n", iter, test_id);
        pid_t pid = fork();

        if (pid < 0) {
            perror("fork");
            return 1;
        }

        if (pid == 0) {
            /* Child: run the test. */
            execute_one(test_id, iter);
            _exit(0);
        }

        /* Parent: wait for child (it will crash). */
        int status = 0;
        waitpid(pid, &status, 0);

        if (WIFSIGNALED(status)) {
            printf("iteration %d: child killed by signal %d\n",
                   iter, WTERMSIG(status));
        } else if (WIFEXITED(status)) {
            printf("iteration %d: child exited with %d\n",
                   iter, WEXITSTATUS(status));
        }
    }

    munmap(scratch, SCRATCH_SIZE);
    printf("harness complete.\n");
    return 0;
}

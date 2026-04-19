/* multithread-crash.c — Crash in one thread while others are running.
 *
 * Showcases: per-thread inspection after a crash.  Four worker threads
 * run busy loops; one dereferences NULL after a delay.  GDB stops all
 * threads on the signal, letting you inspect both the crashed thread
 * and the healthy ones.
 *
 * Compile: gcc -g -O0 -pthread -o /tmp/multithread-crash examples/programs/multithread-crash.c
 *
 * Scheme session — find the crashing thread:
 *   (begin
 *     (load-file "/tmp/multithread-crash")
 *     (run)
 *     (wait-for-stop)           ;; catches SIGSEGV in the crashing thread
 *     (define threads (list-threads))
 *     threads                   ;; shows all threads and their states
 *     (backtrace)               ;; backtrace of the crashing thread
 *     ;; Now inspect a healthy worker
 *     (select-thread 2)
 *     (backtrace)               ;; should show the busy loop
 *     (list-locals))
 */

#include <stdio.h>
#include <pthread.h>
#include <unistd.h>

#define NUM_WORKERS 3

typedef struct WorkerCtx {
    int id;
    volatile int running;
    long counter;
} WorkerCtx;

/* Healthy worker — busy loop counting. */
void *healthy_worker(void *arg) {
    WorkerCtx *ctx = (WorkerCtx *)arg;
    ctx->running = 1;

    while (ctx->running) {
        ctx->counter++;
        /* Burn some CPU so the thread is visible in GDB. */
        for (volatile int i = 0; i < 1000; i++) { }
    }

    printf("worker %d: finished with counter=%ld\n", ctx->id, ctx->counter);
    return NULL;
}

/* Crashing worker — waits briefly then dereferences NULL. */
void *crashing_worker(void *arg) {
    int *doomed = NULL;

    printf("crasher: sleeping before crash...\n");
    usleep(200000);  /* 200ms — let other threads get going. */

    printf("crasher: about to dereference NULL\n");
    *doomed = 0xDEAD;  /* SIGSEGV */

    return NULL;
}

int main(void) {
    pthread_t workers[NUM_WORKERS];
    pthread_t crasher;
    WorkerCtx contexts[NUM_WORKERS];

    printf("spawning %d healthy workers + 1 crasher\n", NUM_WORKERS);

    for (int i = 0; i < NUM_WORKERS; i++) {
        contexts[i].id = i;
        contexts[i].running = 1;
        contexts[i].counter = 0;
        if (pthread_create(&workers[i], NULL, healthy_worker, &contexts[i]) != 0) {
            perror("pthread_create worker");
            return 1;
        }
    }

    if (pthread_create(&crasher, NULL, crashing_worker, NULL) != 0) {
        perror("pthread_create crasher");
        return 1;
    }

    /* Wait for the crasher (it will SIGSEGV before returning). */
    pthread_join(crasher, NULL);

    /* Clean up workers (won't reach here due to crash). */
    for (int i = 0; i < NUM_WORKERS; i++) {
        contexts[i].running = 0;
        pthread_join(workers[i], NULL);
    }

    return 0;
}

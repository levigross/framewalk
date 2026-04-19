/* threads.c — Multiple threads with a shared counter (data race).
 *
 * Showcases: multi-thread debugging.  The Scheme script stops all
 * threads and inspects each one's backtrace and local variables —
 * something painful to do manually but trivial to script.
 *
 * Compile: gcc -g -O0 -pthread -o /tmp/threads examples/programs/threads.c
 *
 * Scheme session — inspect all threads at a breakpoint:
 *   (begin
 *     (load-file "/tmp/threads")
 *     (set-breakpoint "worker")
 *     (run)
 *     (wait-for-stop)
 *
 *     ;; How many threads are alive?
 *     (define thread-info (list-threads))
 *
 *     ;; Get backtrace for the stopped thread
 *     (backtrace))
 *
 * Or inspect a specific thread:
 *   (begin
 *     (load-file "/tmp/threads")
 *     (set-breakpoint "worker")
 *     (run)
 *     (wait-for-stop)
 *     (select-thread 2)
 *     (list-locals))
 */

#include <stdio.h>
#include <stdlib.h>
#include <pthread.h>
#include <unistd.h>

#define NUM_THREADS 4
#define ITERATIONS  1000

/* Shared state — deliberately unprotected (data race). */
int shared_counter = 0;

typedef struct WorkerArgs {
    int id;
    int iterations;
} WorkerArgs;

void *worker(void *arg) {
    WorkerArgs *wa = (WorkerArgs *)arg;
    int local_sum = 0;

    for (int i = 0; i < wa->iterations; i++) {
        /* Deliberate race: read-modify-write without a lock. */
        int val = shared_counter;
        val += 1;
        shared_counter = val;
        local_sum += val;
    }

    printf("thread %d: local_sum = %d\n", wa->id, local_sum);
    return NULL;
}

int main(void) {
    pthread_t threads[NUM_THREADS];
    WorkerArgs args[NUM_THREADS];

    printf("starting %d threads, %d iterations each\n",
           NUM_THREADS, ITERATIONS);

    for (int i = 0; i < NUM_THREADS; i++) {
        args[i].id = i;
        args[i].iterations = ITERATIONS;
        if (pthread_create(&threads[i], NULL, worker, &args[i]) != 0) {
            perror("pthread_create");
            return 1;
        }
    }

    for (int i = 0; i < NUM_THREADS; i++) {
        pthread_join(threads[i], NULL);
    }

    printf("expected counter = %d\n", NUM_THREADS * ITERATIONS);
    printf("actual counter   = %d\n", shared_counter);
    if (shared_counter != NUM_THREADS * ITERATIONS) {
        printf("DATA RACE DETECTED: lost %d increments\n",
               NUM_THREADS * ITERATIONS - shared_counter);
    }

    return 0;
}

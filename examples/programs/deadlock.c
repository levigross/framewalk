/* deadlock.c — Classic ABBA deadlock with two threads and two mutexes.
 *
 * Showcases: debugging a hung program.  Thread 1 locks A then B;
 * thread 2 locks B then A.  The program hangs (does not crash).
 * Use interrupt to break in and inspect per-thread state.
 *
 * Compile: gcc -g -O0 -pthread -o /tmp/deadlock examples/programs/deadlock.c
 *
 * Scheme session — detect the deadlock:
 *   (begin
 *     (load-file "/tmp/deadlock")
 *     (run)
 *     ;; Wait a moment for the deadlock to form, then interrupt
 *     (interrupt)
 *     (wait-for-stop)
 *     (list-threads)            ;; both threads should be alive
 *     ;; Inspect thread 2
 *     (select-thread 2)
 *     (backtrace)               ;; should show pthread_mutex_lock
 *     (list-locals)
 *     ;; Inspect thread 3
 *     (select-thread 3)
 *     (backtrace)               ;; also stuck in pthread_mutex_lock
 *     (list-locals))
 */

#include <stdio.h>
#include <pthread.h>
#include <unistd.h>

pthread_mutex_t mutex_a = PTHREAD_MUTEX_INITIALIZER;
pthread_mutex_t mutex_b = PTHREAD_MUTEX_INITIALIZER;

void *thread_one(void *arg) {
    (void)arg;
    printf("thread 1: locking mutex_a\n");
    pthread_mutex_lock(&mutex_a);

    /* Small sleep to ensure thread 2 grabs mutex_b first. */
    usleep(100000);  /* 100ms */

    printf("thread 1: locking mutex_b (will block)\n");
    pthread_mutex_lock(&mutex_b);  /* Deadlock: thread 2 holds mutex_b. */

    printf("thread 1: acquired both (should never reach here)\n");
    pthread_mutex_unlock(&mutex_b);
    pthread_mutex_unlock(&mutex_a);
    return NULL;
}

void *thread_two(void *arg) {
    (void)arg;
    printf("thread 2: locking mutex_b\n");
    pthread_mutex_lock(&mutex_b);

    usleep(100000);  /* 100ms */

    printf("thread 2: locking mutex_a (will block)\n");
    pthread_mutex_lock(&mutex_a);  /* Deadlock: thread 1 holds mutex_a. */

    printf("thread 2: acquired both (should never reach here)\n");
    pthread_mutex_unlock(&mutex_a);
    pthread_mutex_unlock(&mutex_b);
    return NULL;
}

int main(void) {
    pthread_t t1, t2;

    printf("spawning threads to create ABBA deadlock...\n");

    if (pthread_create(&t1, NULL, thread_one, NULL) != 0) {
        perror("pthread_create t1");
        return 1;
    }
    if (pthread_create(&t2, NULL, thread_two, NULL) != 0) {
        perror("pthread_create t2");
        return 1;
    }

    /* Wait forever — the threads are deadlocked. */
    pthread_join(t1, NULL);
    pthread_join(t2, NULL);

    printf("done (should never reach here)\n");
    return 0;
}

/* memory-leak.c — Allocator that leaks under certain conditions.
 *
 * Showcases: allocation tracking.  A simple pool allocator hands out
 * blocks.  Some code paths forget to free.  The Scheme script can set
 * breakpoints on alloc/free, count calls, and compare — revealing the
 * mismatch.
 *
 * Compile: gcc -g -O0 -o /tmp/memory-leak examples/programs/memory-leak.c
 *
 * Scheme session — count allocs vs frees:
 *   (begin
 *     (load-file "/tmp/memory-leak")
 *     (set-breakpoint "pool_alloc")
 *     (set-breakpoint "pool_free")
 *     (run)
 *
 *     (define alloc-count 0)
 *     (define free-count 0)
 *
 *     ;; Run through the program, counting allocs and frees.
 *     (define (tally max-stops)
 *       (let loop ((i 0))
 *         (if (>= i max-stops) 'done
 *             (begin
 *               (wait-for-stop)
 *               (let ((bt (backtrace)))
 *                 ;; Check which breakpoint we hit by looking at the
 *                 ;; innermost frame's function name.
 *                 (let* ((top-frame (car bt))
 *                        (func (result-field "func" top-frame)))
 *                   (cond
 *                     ((equal? func "pool_alloc") (set! alloc-count (+ alloc-count 1)))
 *                     ((equal? func "pool_free")  (set! free-count (+ free-count 1))))
 *                   (cont)
 *                   (loop (+ i 1))))))))
 *
 *     (tally 30)
 *     (list "allocs" alloc-count "frees" free-count
 *           "leaked" (- alloc-count free-count)))
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define BLOCK_SIZE 64

typedef struct Block {
    int  id;
    int  in_use;
    char data[BLOCK_SIZE];
} Block;

static int next_id = 1;

Block *pool_alloc(void) {
    Block *b = malloc(sizeof(Block));
    if (!b) return NULL;
    b->id = next_id++;
    b->in_use = 1;
    memset(b->data, 0, BLOCK_SIZE);
    return b;
}

void pool_free(Block *b) {
    if (b) {
        b->in_use = 0;
        free(b);
    }
}

/* Process that properly frees. */
void good_process(void) {
    Block *a = pool_alloc();
    Block *b = pool_alloc();
    snprintf(a->data, BLOCK_SIZE, "hello");
    snprintf(b->data, BLOCK_SIZE, "world");
    printf("good_process: %s %s\n", a->data, b->data);
    pool_free(a);
    pool_free(b);
}

/* Process that leaks on an error path. */
void leaky_process(int trigger_error) {
    Block *a = pool_alloc();
    Block *b = pool_alloc();
    Block *c = pool_alloc();

    snprintf(a->data, BLOCK_SIZE, "alpha");
    snprintf(b->data, BLOCK_SIZE, "bravo");
    snprintf(c->data, BLOCK_SIZE, "charlie");

    if (trigger_error) {
        printf("leaky_process: error path — forgetting to free b and c\n");
        pool_free(a);
        /* BUG: b and c are leaked. */
        return;
    }

    printf("leaky_process: %s %s %s\n", a->data, b->data, c->data);
    pool_free(a);
    pool_free(b);
    pool_free(c);
}

int main(void) {
    good_process();
    leaky_process(0);  /* clean path */
    good_process();
    leaky_process(1);  /* leaky path */
    good_process();

    printf("total allocations made: %d\n", next_id - 1);
    return 0;
}

/* ring-buffer.c — Fixed-size ring buffer with an off-by-one bug.
 *
 * Showcases: bug hunting.  The ring buffer works for small sequences
 * but wraps incorrectly when it fills up.  The Scheme script can push
 * values, inspect internal state (head, tail, count), and pinpoint
 * exactly when the invariant breaks.
 *
 * Compile: gcc -g -O0 -o /tmp/ring-buffer examples/programs/ring-buffer.c
 *
 * Scheme session — push values and watch invariants:
 *   (begin
 *     (load-file "/tmp/ring-buffer")
 *     (set-breakpoint "rb_push")
 *     (run)
 *     (wait-for-stop)
 *
 *     (define (watch-pushes n)
 *       (let loop ((i 0) (acc '()))
 *         (if (>= i n) (reverse acc)
 *             (let ((head  (inspect "rb->head"))
 *                   (tail  (inspect "rb->tail"))
 *                   (count (inspect "rb->count"))
 *                   (cap   (inspect "rb->capacity"))
 *                   (val   (inspect "value")))
 *               (cont)
 *               (wait-for-stop)
 *               (loop (+ i 1)
 *                     (cons (list val head tail count) acc))))))
 *
 *     (watch-pushes 12))
 */

#include <stdio.h>
#include <stdlib.h>

typedef struct RingBuffer {
    int *data;
    int  head;      /* next write position */
    int  tail;      /* next read position  */
    int  count;     /* current occupancy   */
    int  capacity;
} RingBuffer;

RingBuffer *rb_create(int capacity) {
    RingBuffer *rb = malloc(sizeof(RingBuffer));
    rb->data = calloc(capacity, sizeof(int));
    rb->head = 0;
    rb->tail = 0;
    rb->count = 0;
    rb->capacity = capacity;
    return rb;
}

int rb_push(RingBuffer *rb, int value) {
    if (rb->count >= rb->capacity) {
        return -1;  /* full */
    }
    rb->data[rb->head] = value;
    /* BUG: should be (head + 1) % capacity, but uses capacity + 1 */
    rb->head = (rb->head + 1) % (rb->capacity + 1);
    rb->count++;
    return 0;
}

int rb_pop(RingBuffer *rb, int *out) {
    if (rb->count <= 0) {
        return -1;  /* empty */
    }
    *out = rb->data[rb->tail];
    rb->tail = (rb->tail + 1) % rb->capacity;
    rb->count--;
    return 0;
}

int rb_peek(RingBuffer *rb) {
    if (rb->count <= 0) return -1;
    return rb->data[rb->tail];
}

void rb_free(RingBuffer *rb) {
    free(rb->data);
    free(rb);
}

int main(void) {
    RingBuffer *rb = rb_create(4);

    /* Push and pop a few values — works fine for small usage. */
    for (int i = 1; i <= 3; i++) {
        rb_push(rb, i * 100);
        printf("pushed %d, head=%d tail=%d count=%d\n",
               i * 100, rb->head, rb->tail, rb->count);
    }

    int val;
    rb_pop(rb, &val);
    printf("popped %d\n", val);
    rb_pop(rb, &val);
    printf("popped %d\n", val);

    /* Now fill it again — the bug triggers when head wraps. */
    for (int i = 4; i <= 10; i++) {
        int rc = rb_push(rb, i * 100);
        if (rc != 0) {
            printf("push %d FAILED (full), head=%d tail=%d count=%d\n",
                   i * 100, rb->head, rb->tail, rb->count);
        } else {
            printf("pushed %d, head=%d tail=%d count=%d\n",
                   i * 100, rb->head, rb->tail, rb->count);
        }
    }

    /* Drain */
    while (rb_pop(rb, &val) == 0) {
        printf("popped %d\n", val);
    }

    rb_free(rb);
    return 0;
}

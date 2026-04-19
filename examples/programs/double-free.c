/* double-free.c — glibc detects double-free and aborts.
 *
 * Showcases: SIGABRT handling and backtrace through the C library
 * allocator.  glibc's malloc detects the double-free and calls abort(),
 * producing a backtrace that descends through __libc_malloc/abort.
 *
 * Compile: gcc -g -O0 -o /tmp/double-free examples/programs/double-free.c
 *
 * Scheme session — catch the abort and inspect:
 *   (begin
 *     (load-file "/tmp/double-free")
 *     (run)
 *     (wait-for-stop)           ;; catches SIGABRT from glibc
 *     (backtrace)               ;; shows frames through glibc's abort path
 *     ;; Walk up to the application frame that triggered the second free
 *     (select-frame 4)          ;; adjust based on actual depth
 *     (list-locals))
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct Buffer {
    char  *data;
    size_t size;
} Buffer;

Buffer *buffer_create(size_t size) {
    Buffer *buf = malloc(sizeof(Buffer));
    if (!buf) return NULL;
    buf->data = malloc(size);
    buf->size = size;
    if (buf->data) {
        memset(buf->data, 'X', size);
    }
    return buf;
}

void buffer_destroy(Buffer *buf) {
    if (buf) {
        free(buf->data);
        free(buf);
    }
}

/* Simulates a cleanup function that doesn't know the buffer
 * was already freed elsewhere. */
void cleanup_resources(Buffer *buf) {
    printf("cleanup: freeing buffer at %p\n", (void *)buf);
    buffer_destroy(buf);  /* Second free — glibc will abort. */
}

int main(void) {
    Buffer *buf = buffer_create(128);
    printf("created buffer: %p (data: %p, size: %zu)\n",
           (void *)buf, (void *)buf->data, buf->size);

    /* First free — legitimate. */
    printf("destroying buffer...\n");
    buffer_destroy(buf);

    /* Second free — bug.  glibc detects this and calls abort(). */
    printf("cleanup (double-free incoming)...\n");
    cleanup_resources(buf);

    printf("done (should not reach here)\n");
    return 0;
}

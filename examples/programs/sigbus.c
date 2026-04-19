/* sigbus.c — SIGBUS from accessing past the end of an mmap'd file.
 *
 * Showcases: unusual signal handling.  mmapping a file and accessing
 * past the file's backing produces SIGBUS (not SIGSEGV).  This is a
 * common real-world crash in programs that memory-map files.
 *
 * Compile: gcc -g -O0 -o /tmp/sigbus examples/programs/sigbus.c
 *
 * Scheme session — inspect the mmap region:
 *   (begin
 *     (load-file "/tmp/sigbus")
 *     (run)
 *     (wait-for-stop)           ;; catches SIGBUS
 *     (backtrace)
 *     (inspect "map_base")      ;; the mmap'd address
 *     (inspect "file_size")     ;; the actual file size (1 byte)
 *     (inspect "access_offset") ;; where we tried to read
 *     (list-locals))
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <fcntl.h>
#include <unistd.h>
#include <sys/mman.h>
#include <sys/stat.h>

#define MAP_LENGTH 4096

void access_mapped_file(char *map_base, size_t file_size) {
    /* Reading within the file's backing is fine. */
    printf("byte at offset 0: 0x%02x\n", (unsigned char)map_base[0]);

    /* Accessing past the file's backing but within the mapped page
     * may or may not fault (kernel zero-fills the page remainder).
     * Accessing into the next page past the file backing triggers SIGBUS. */
    size_t access_offset = MAP_LENGTH;
    printf("accessing offset %zu (file is only %zu bytes)...\n",
           access_offset, file_size);

    /* This triggers SIGBUS — the page beyond the file's backing
     * has no data to serve. */
    char val = map_base[access_offset];
    printf("value: 0x%02x (should not reach here)\n", (unsigned char)val);
}

int main(void) {
    const char *path = "/tmp/sigbus-testfile";

    /* Create a tiny file — just 1 byte. */
    int fd = open(path, O_CREAT | O_RDWR | O_TRUNC, 0644);
    if (fd < 0) { perror("open"); return 1; }

    char byte = 0x42;
    if (write(fd, &byte, 1) != 1) { perror("write"); close(fd); return 1; }

    size_t file_size = 1;

    /* Map more than the file's size.  We ask for 2 pages but the file
     * only backs 1 byte (the rest of the first page is zero-filled). */
    char *map_base = mmap(NULL, MAP_LENGTH * 2, PROT_READ, MAP_SHARED, fd, 0);
    if (map_base == MAP_FAILED) {
        perror("mmap");
        close(fd);
        return 1;
    }
    close(fd);

    printf("mapped %d bytes at %p (file is %zu bytes)\n",
           MAP_LENGTH * 2, (void *)map_base, file_size);

    access_mapped_file(map_base, file_size);

    munmap(map_base, MAP_LENGTH * 2);
    unlink(path);
    return 0;
}

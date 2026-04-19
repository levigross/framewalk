/* use-after-free.c — Heap use-after-free producing a crash.
 *
 * Showcases: memory inspection on corrupted heap state.  A struct is
 * allocated, populated, freed, and then accessed through the dangling
 * pointer.  Depending on the allocator state this produces SIGSEGV or
 * silent corruption — either way, the debug session is interesting.
 *
 * Compile: gcc -g -O0 -o /tmp/use-after-free examples/programs/use-after-free.c
 *
 * Scheme session — inspect the dangling pointer:
 *   (begin
 *     (load-file "/tmp/use-after-free")
 *     (set-breakpoint "use_dangling")
 *     (run)
 *     (wait-for-stop)
 *     (inspect "rec")           ;; shows the dangling pointer address
 *     (inspect "rec->name")     ;; may show garbage or crash
 *     (list-locals)
 *     (cont)
 *     (wait-for-stop)           ;; catches the crash
 *     (backtrace))
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct Record {
    int    id;
    char  *name;
    double score;
} Record;

Record *create_record(int id, const char *name, double score) {
    Record *r = malloc(sizeof(Record));
    if (!r) return NULL;
    r->id = id;
    r->name = strdup(name);
    r->score = score;
    return r;
}

void destroy_record(Record *r) {
    if (r) {
        free(r->name);
        r->name = NULL;
        free(r);
    }
}

/* Called with a dangling pointer — the record was already freed. */
void use_dangling(Record *rec) {
    printf("accessing freed record at %p\n", (void *)rec);

    /* Force the allocator to recycle the memory so the old values
     * are overwritten with heap metadata or new allocations. */
    void *junk1 = malloc(sizeof(Record));
    void *junk2 = malloc(sizeof(Record));
    memset(junk1, 0xAA, sizeof(Record));
    memset(junk2, 0xBB, sizeof(Record));

    /* Now read through the dangling pointer — the data is corrupted. */
    printf("rec->id    = %d\n", rec->id);
    printf("rec->score = %f\n", rec->score);

    /* This dereferences rec->name which is now garbage — crash. */
    printf("rec->name  = %s\n", rec->name);

    free(junk1);
    free(junk2);
}

int main(void) {
    Record *r = create_record(42, "Alice", 98.6);
    printf("created: id=%d name=%s score=%.1f\n", r->id, r->name, r->score);

    destroy_record(r);
    printf("record freed.\n");

    /* Bug: use the freed pointer. */
    use_dangling(r);

    return 0;
}

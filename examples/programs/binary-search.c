/* binary-search.c — Binary search with full decision trace.
 *
 * Showcases: algorithm visualisation.  Scheme breaks on each
 * iteration of the search loop and collects (low, mid, high, decision)
 * tuples — producing a complete trace of how the search narrows in.
 *
 * Compile: gcc -g -O0 -o /tmp/binary-search examples/programs/binary-search.c
 *
 * Scheme session — trace every decision:
 *   (begin
 *     (load-file "/tmp/binary-search")
 *     (set-breakpoint "binary_search.c:24")   ;; the comparison line
 *     (run)
 *     (wait-for-stop)
 *
 *     (define (trace-search max-steps)
 *       (let loop ((i 0) (acc '()))
 *         (if (>= i max-steps) (reverse acc)
 *             (let ((low  (inspect "low"))
 *                   (mid  (inspect "mid"))
 *                   (high (inspect "high"))
 *                   (mid-val (inspect "arr[mid]"))
 *                   (target  (inspect "target")))
 *               (cont)
 *               (wait-for-stop)
 *               (loop (+ i 1)
 *                     (cons (list low mid high mid-val target) acc))))))
 *
 *     (trace-search 20))
 */

#include <stdio.h>

int binary_search(int *arr, int len, int target) {
    int low  = 0;
    int high = len - 1;

    while (low <= high) {
        int mid = low + (high - low) / 2;

        if (arr[mid] == target) {    /* line for breakpoint */
            return mid;
        } else if (arr[mid] < target) {
            low = mid + 1;
        } else {
            high = mid - 1;
        }
    }

    return -1;  /* not found */
}

int main(void) {
    int data[] = {2, 5, 8, 12, 16, 23, 38, 42, 56, 72, 91, 100};
    int len = sizeof(data) / sizeof(data[0]);

    int targets[] = {23, 42, 99, 2, 100, 50};
    int n_targets = sizeof(targets) / sizeof(targets[0]);

    for (int i = 0; i < n_targets; i++) {
        int idx = binary_search(data, len, targets[i]);
        if (idx >= 0) {
            printf("found %d at index %d\n", targets[i], idx);
        } else {
            printf("%d not found\n", targets[i]);
        }
    }

    return 0;
}

/* sort-visual.c — Insertion sort with per-swap visibility.
 *
 * Showcases: algorithm tracing.  Break inside the inner loop and
 * collect the full array state at each swap to build a visual
 * trace of the sort in a single Scheme call.
 *
 * Compile: gcc -g -O0 -o /tmp/sort-visual examples/programs/sort-visual.c
 *
 * Scheme session — collect array snapshots at each swap:
 *   (begin
 *     (load-file "/tmp/sort-visual")
 *     (set-breakpoint "do_swap")
 *     (run)
 *     (wait-for-stop)
 *     (define (collect-swaps max-stops)
 *       (let loop ((i 0) (acc '()))
 *         (if (>= i max-stops) (reverse acc)
 *             (let ((snapshot (list
 *                    (inspect "arr[0]") (inspect "arr[1]")
 *                    (inspect "arr[2]") (inspect "arr[3]")
 *                    (inspect "arr[4]") (inspect "arr[5]")
 *                    (inspect "arr[6]") (inspect "arr[7]"))))
 *               (cont)
 *               (wait-for-stop)
 *               (loop (+ i 1) (cons snapshot acc))))))
 *     (collect-swaps 10))
 */

#include <stdio.h>

#define N 8

/* Separate function so we can breakpoint just the swap. */
void do_swap(int *arr, int i, int j) {
    int tmp = arr[i];
    arr[i] = arr[j];
    arr[j] = tmp;
}

void print_array(int *arr, int len) {
    for (int i = 0; i < len; i++) {
        printf("%3d", arr[i]);
    }
    printf("\n");
}

void insertion_sort(int *arr, int len) {
    for (int i = 1; i < len; i++) {
        int j = i;
        while (j > 0 && arr[j - 1] > arr[j]) {
            do_swap(arr, j - 1, j);
            j--;
        }
    }
}

int main(void) {
    int arr[N] = {42, 17, 93, 5, 28, 71, 8, 56};

    printf("before: ");
    print_array(arr, N);

    insertion_sort(arr, N);

    printf("after:  ");
    print_array(arr, N);

    return 0;
}

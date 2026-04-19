/* Test program for debugging with framewalk-mcp scheme_eval.
 *
 * Compile with debug info:
 *   gcc -g -O0 -o /tmp/hello examples/hello.c
 *
 * Then point scheme_eval at it:
 *   (load-file "/tmp/hello")
 *   (run-to "main")
 *   (inspect "argc")
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

/* --- Recursive --- */

int factorial(int n) {
    if (n <= 1) return 1;
    return n * factorial(n - 1);
}

int fibonacci(int n) {
    if (n <= 0) return 0;
    if (n == 1) return 1;
    return fibonacci(n - 1) + fibonacci(n - 2);
}

/* --- Structs --- */

typedef struct Point {
    double x;
    double y;
} Point;

typedef struct Node {
    int value;
    struct Node *next;
} Node;

Point make_point(double x, double y) {
    Point p = { .x = x, .y = y };
    return p;
}

double distance(Point a, Point b) {
    double dx = a.x - b.x;
    double dy = a.y - b.y;
    return dx * dx + dy * dy; /* squared — avoids needing -lm */
}

/* --- Linked list --- */

Node *list_push(Node *head, int value) {
    Node *node = malloc(sizeof(Node));
    node->value = value;
    node->next = head;
    return node;
}

int list_sum(Node *head) {
    int total = 0;
    for (Node *cur = head; cur != NULL; cur = cur->next) {
        total += cur->value;
    }
    return total;
}

void list_free(Node *head) {
    while (head) {
        Node *tmp = head;
        head = head->next;
        free(tmp);
    }
}

/* --- Array --- */

int sum_array(int *arr, int len) {
    int total = 0;
    for (int i = 0; i < len; i++) {
        total += arr[i];
    }
    return total;
}

void bubble_sort(int *arr, int len) {
    for (int i = 0; i < len - 1; i++) {
        for (int j = 0; j < len - i - 1; j++) {
            if (arr[j] > arr[j + 1]) {
                int tmp = arr[j];
                arr[j] = arr[j + 1];
                arr[j + 1] = tmp;
            }
        }
    }
}

/* --- String --- */

void reverse_string(char *s) {
    int len = strlen(s);
    for (int i = 0; i < len / 2; i++) {
        char tmp = s[i];
        s[i] = s[len - 1 - i];
        s[len - 1 - i] = tmp;
    }
}

/* --- Main --- */

int main(int argc, char **argv) {
    printf("hello from framewalk test program\n");

    /* Arrays */
    int numbers[] = {5, 3, 8, 1, 9, 2, 7, 4, 6, 10};
    int len = sizeof(numbers) / sizeof(numbers[0]);

    bubble_sort(numbers, len);
    int total = sum_array(numbers, len);
    printf("sorted sum = %d\n", total);

    /* Recursion */
    int fact = factorial(6);
    printf("6! = %d\n", fact);

    int fib = fibonacci(8);
    printf("fib(8) = %d\n", fib);

    /* Structs */
    Point origin = make_point(0.0, 0.0);
    Point target = make_point(3.0, 4.0);
    double dist = distance(origin, target);
    printf("distance^2 = %.1f\n", dist);

    /* Linked list */
    Node *list = NULL;
    for (int i = 1; i <= 5; i++) {
        list = list_push(list, i * 10);
    }
    int lsum = list_sum(list);
    printf("list sum = %d\n", lsum);
    list_free(list);

    /* String */
    char greeting[] = "framewalk";
    reverse_string(greeting);
    printf("reversed = %s\n", greeting);

    return 0;
}

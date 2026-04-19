/* linked-list.c — Build, walk, and corrupt a linked list.
 *
 * Showcases: pointer chasing in Scheme.  The LLM writes a Scheme loop
 * that follows ->next pointers, collecting values until it hits NULL
 * or a cycle — something that would take many individual tool calls.
 *
 * Compile: gcc -g -O0 -o /tmp/linked-list examples/programs/linked-list.c
 *
 * Scheme session — walk the list by following pointers:
 *   (begin
 *     (load-file "/tmp/linked-list")
 *     (set-breakpoint "walk_list")
 *     (run)
 *     (wait-for-stop)
 *
 *     ;; Walk the linked list by inspecting head, then ->next, etc.
 *     ;; We use GDB pointer syntax to follow the chain.
 *     (define (walk-list-from expr max-depth)
 *       (let loop ((i 0) (cur expr) (acc '()))
 *         (if (>= i max-depth) (reverse acc)
 *             (let ((val (inspect (string-append cur "->value")))
 *                   (nxt (inspect (string-append cur "->next"))))
 *               (if (equal? (result-field "value" nxt) "0x0")
 *                   (reverse (cons val acc))
 *                   (loop (+ i 1)
 *                         (string-append "(" cur "->next)")
 *                         (cons val acc)))))))
 *
 *     (walk-list-from "head" 20))
 */

#include <stdio.h>
#include <stdlib.h>

typedef struct Node {
    int          value;
    struct Node *next;
} Node;

Node *make_node(int value) {
    Node *n = malloc(sizeof(Node));
    if (!n) { perror("malloc"); exit(1); }
    n->value = value;
    n->next  = NULL;
    return n;
}

/* Build a list: 1 -> 2 -> 3 -> ... -> count -> NULL */
Node *build_list(int count) {
    Node *head = NULL;
    Node *tail = NULL;
    for (int i = 1; i <= count; i++) {
        Node *n = make_node(i * 10);
        if (!head) {
            head = n;
            tail = n;
        } else {
            tail->next = n;
            tail = n;
        }
    }
    return head;
}

/* Walk and print. */
void walk_list(Node *head) {
    printf("list: ");
    for (Node *cur = head; cur; cur = cur->next) {
        printf("%d -> ", cur->value);
    }
    printf("NULL\n");
}

/* Intentionally corrupt: create a cycle. */
void corrupt_list(Node *head) {
    if (!head) return;
    Node *last = head;
    while (last->next) last = last->next;
    /* Point the last node back to the second node — a cycle. */
    if (head->next) {
        last->next = head->next;
    }
}

/* Walk a corrupted list — will loop forever if unchecked. */
int count_until_cycle(Node *head, int max) {
    int count = 0;
    for (Node *cur = head; cur && count < max; cur = cur->next) {
        count++;
    }
    return count;
}

void free_list(Node *head) {
    while (head) {
        Node *tmp = head;
        head = head->next;
        free(tmp);
    }
}

int main(void) {
    Node *list = build_list(8);
    walk_list(list);

    /* Now corrupt it and see what happens. */
    corrupt_list(list);
    int n = count_until_cycle(list, 100);
    printf("walked %d nodes before giving up (cycle detected)\n", n);

    /* Don't free — it's corrupted (would loop). */
    return 0;
}

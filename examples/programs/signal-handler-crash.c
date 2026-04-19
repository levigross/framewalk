/* signal-handler-crash.c — Crash inside a signal handler.
 *
 * Showcases: signal trampoline frames in the backtrace.  A SIGUSR1
 * handler dereferences NULL, producing a SIGSEGV from within the
 * handler.  The backtrace shows a signal trampoline frame
 * (<signal handler called>) between the handler and main.
 *
 * Compile: gcc -g -O0 -o /tmp/signal-handler-crash examples/programs/signal-handler-crash.c
 *
 * Scheme session — inspect the signal frames:
 *   (begin
 *     (load-file "/tmp/signal-handler-crash")
 *     (run)
 *     (wait-for-stop)           ;; catches SIGSEGV inside the handler
 *     (backtrace)               ;; shows: handler -> <signal handler called> -> raise -> main
 *     ;; Select the frame below the signal trampoline
 *     (select-frame 0)
 *     (list-locals)             ;; handler's local variables
 *     ;; Select the frame above the trampoline (in main)
 *     (select-frame 4)          ;; adjust based on actual depth
 *     (list-locals))            ;; main's local variables
 */

#include <stdio.h>
#include <signal.h>
#include <string.h>

void handler_helper(int signo) {
    printf("handler_helper: processing signal %d\n", signo);

    /* Crash inside the signal handler. */
    int *null = NULL;
    *null = signo;  /* SIGSEGV from within the SIGUSR1 handler */
}

void sigusr1_handler(int signo) {
    printf("sigusr1_handler: received signal %d\n", signo);
    handler_helper(signo);
}

void do_work(void) {
    printf("do_work: sending SIGUSR1 to self\n");
    raise(SIGUSR1);
    printf("do_work: returned from raise (should not reach here)\n");
}

int main(void) {
    struct sigaction sa;
    memset(&sa, 0, sizeof(sa));
    sa.sa_handler = sigusr1_handler;
    sigemptyset(&sa.sa_mask);
    /* Do not set SA_RESETHAND — we want the handler to stay installed. */

    if (sigaction(SIGUSR1, &sa, NULL) != 0) {
        perror("sigaction");
        return 1;
    }

    printf("signal handler installed for SIGUSR1\n");
    do_work();

    return 0;
}

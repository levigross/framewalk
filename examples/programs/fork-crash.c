/* fork-crash.c — Fork followed by crash in the child process.
 *
 * Showcases: multi-process debugging.  The parent forks, the child
 * dereferences NULL after a brief delay, and the parent waits.
 * Tests GDB's follow-fork-mode and framewalk's process/inferior handling.
 *
 * Compile: gcc -g -O0 -o /tmp/fork-crash examples/programs/fork-crash.c
 *
 * Scheme session — follow the child into the crash:
 *   (begin
 *     (load-file "/tmp/fork-crash")
 *     ;; Tell GDB to follow the child on fork
 *     (mi "-gdb-set follow-fork-mode child")
 *     (run)
 *     (wait-for-stop)           ;; catches SIGSEGV in child
 *     (backtrace)               ;; child's crash backtrace
 *     (inspect "child_id")      ;; shows the child's identifier
 *     (list-locals))
 *
 * Alternative — stay with parent:
 *   (begin
 *     (load-file "/tmp/fork-crash")
 *     (mi "-gdb-set follow-fork-mode parent")
 *     (set-breakpoint "parent_wait")
 *     (run)
 *     (wait-for-stop)
 *     (inspect "status"))       ;; child's exit status from waitpid
 */

#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <sys/wait.h>

void child_work(int child_id) {
    printf("child[%d] (pid=%d): starting work\n", child_id, getpid());
    usleep(100000);  /* 100ms */

    printf("child[%d]: about to crash\n", child_id);
    int *null = NULL;
    *null = child_id;  /* SIGSEGV */
}

void parent_wait(pid_t child_pid) {
    int status = 0;
    printf("parent (pid=%d): waiting for child %d\n", getpid(), child_pid);
    waitpid(child_pid, &status, 0);

    if (WIFSIGNALED(status)) {
        printf("parent: child killed by signal %d\n", WTERMSIG(status));
    } else if (WIFEXITED(status)) {
        printf("parent: child exited with status %d\n", WEXITSTATUS(status));
    }
}

int main(void) {
    printf("parent (pid=%d): forking...\n", getpid());

    pid_t pid = fork();

    if (pid < 0) {
        perror("fork");
        return 1;
    }

    if (pid == 0) {
        /* Child process. */
        child_work(42);
        _exit(0);  /* Won't reach due to crash. */
    }

    /* Parent process. */
    parent_wait(pid);
    return 0;
}

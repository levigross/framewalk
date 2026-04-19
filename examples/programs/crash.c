/* crash.c — Null-pointer dereference buried in a call chain.
 *
 * Showcases: crash investigation.  The Scheme script runs the program,
 * catches the SIGSEGV, inspects the backtrace to find which function
 * dereferenced NULL, and reads the local variables at the crash site.
 *
 * Compile: gcc -g -O0 -o /tmp/crash examples/programs/crash.c
 *
 * Scheme session:
 *   (begin
 *     (load-file "/tmp/crash")
 *     (run)
 *     (wait-for-stop)       ;; catches SIGSEGV
 *     (backtrace))          ;; shows the full call chain to the crash
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef struct Config {
    char *name;
    char *value;
} Config;

typedef struct App {
    Config *config;
    int    initialized;
} App;

/* Read config->name — crashes if config is NULL. */
const char *get_config_name(Config *config) {
    return config->name;  /* <-- crash here when config == NULL */
}

/* Delegates to get_config_name. */
void print_config(App *app) {
    const char *name = get_config_name(app->config);
    printf("config: %s\n", name);
}

/* Forgot to initialize app->config. */
void run_app(App *app) {
    app->initialized = 1;
    /* Oops: never set app->config — it's NULL. */
    print_config(app);
}

int main(void) {
    App app;
    memset(&app, 0, sizeof(app));

    printf("starting app...\n");
    run_app(&app);
    printf("done\n");

    return 0;
}

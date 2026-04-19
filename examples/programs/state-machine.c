/* state-machine.c — Simple HTTP request-line tokeniser.
 *
 * Showcases: tracing state transitions.  The parser walks a byte
 * buffer character by character, transitioning between enum states.
 * Scheme can break on each transition and collect a trace showing
 * exactly how the parser processes the input.
 *
 * Compile: gcc -g -O0 -o /tmp/state-machine examples/programs/state-machine.c
 *
 * Scheme session — collect state transitions:
 *   (begin
 *     (load-file "/tmp/state-machine")
 *     (set-breakpoint "transition")
 *     (run)
 *     (wait-for-stop)
 *
 *     (define (collect-transitions n)
 *       (let loop ((i 0) (acc '()))
 *         (if (>= i n) (reverse acc)
 *             (let ((from-state (inspect "from"))
 *                   (to-state   (inspect "to"))
 *                   (ch         (inspect "trigger")))
 *               (cont)
 *               (wait-for-stop)
 *               (loop (+ i 1)
 *                     (cons (list from-state to-state ch) acc))))))
 *
 *     (collect-transitions 10))
 */

#include <stdio.h>
#include <string.h>
#include <ctype.h>

typedef enum {
    STATE_METHOD,       /* Reading the HTTP method (GET, POST, ...) */
    STATE_SP_AFTER_METHOD,
    STATE_URI,          /* Reading the request URI */
    STATE_SP_AFTER_URI,
    STATE_VERSION,      /* Reading HTTP/1.1 */
    STATE_DONE,
    STATE_ERROR,
} ParseState;

const char *state_name(ParseState s) {
    switch (s) {
        case STATE_METHOD:          return "METHOD";
        case STATE_SP_AFTER_METHOD: return "SP_AFTER_METHOD";
        case STATE_URI:             return "URI";
        case STATE_SP_AFTER_URI:    return "SP_AFTER_URI";
        case STATE_VERSION:         return "VERSION";
        case STATE_DONE:            return "DONE";
        case STATE_ERROR:           return "ERROR";
    }
    return "?";
}

/* Separate function for easy breakpointing. */
void transition(ParseState from, ParseState to, char trigger) {
    printf("  %s -> %s  (char '%c' / 0x%02x)\n",
           state_name(from), state_name(to),
           isprint(trigger) ? trigger : '.', (unsigned char)trigger);
}

typedef struct ParseResult {
    char method[16];
    char uri[256];
    char version[16];
    ParseState final_state;
} ParseResult;

ParseResult parse_request_line(const char *input) {
    ParseResult result;
    memset(&result, 0, sizeof(result));

    ParseState state = STATE_METHOD;
    int mi = 0, ui = 0, vi = 0;

    for (int pos = 0; input[pos] != '\0'; pos++) {
        char ch = input[pos];
        ParseState prev = state;

        switch (state) {
            case STATE_METHOD:
                if (ch == ' ') {
                    state = STATE_SP_AFTER_METHOD;
                } else if (mi < 15) {
                    result.method[mi++] = ch;
                }
                break;

            case STATE_SP_AFTER_METHOD:
                if (ch != ' ') {
                    state = STATE_URI;
                    if (ui < 255) result.uri[ui++] = ch;
                }
                break;

            case STATE_URI:
                if (ch == ' ') {
                    state = STATE_SP_AFTER_URI;
                } else if (ui < 255) {
                    result.uri[ui++] = ch;
                }
                break;

            case STATE_SP_AFTER_URI:
                if (ch != ' ') {
                    state = STATE_VERSION;
                    if (vi < 15) result.version[vi++] = ch;
                }
                break;

            case STATE_VERSION:
                if (ch == '\r' || ch == '\n') {
                    state = STATE_DONE;
                } else if (vi < 15) {
                    result.version[vi++] = ch;
                }
                break;

            case STATE_DONE:
            case STATE_ERROR:
                break;
        }

        if (state != prev) {
            transition(prev, state, ch);
        }
    }

    if (state == STATE_VERSION) {
        transition(state, STATE_DONE, '\0');
        state = STATE_DONE;
    }

    result.final_state = state;
    return result;
}

int main(void) {
    const char *requests[] = {
        "GET /index.html HTTP/1.1\r\n",
        "POST /api/data HTTP/2\r\n",
        "DELETE /users/42 HTTP/1.0\r\n",
    };

    for (int i = 0; i < 3; i++) {
        printf("parsing: %s", requests[i]);
        ParseResult r = parse_request_line(requests[i]);
        printf("  => method='%s' uri='%s' version='%s' state=%s\n\n",
               r.method, r.uri, r.version, state_name(r.final_state));
    }

    return 0;
}

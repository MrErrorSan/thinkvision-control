/* launchwait: exec a program and report ms until an X11 window with the given
 * title is viewable (mapped). Polls the window tree every 2 ms, so resolution is
 * a few ms (xdotool search --sync polls only every 500 ms).
 * Usage: launchwait <title> <timeout_s> <cmd> [args...]
 * Prints "<ms> <pid>" on success; the child is left running for the caller.
 * Build: cc -O2 -o launchwait launchwait.c -lX11 */
#define _GNU_SOURCE
#include <X11/Xlib.h>
#include <X11/Xatom.h>
#include <X11/Xutil.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

static int ignore_err(Display *d, XErrorEvent *e) { (void)d; (void)e; return 0; }
static Atom net_wm_name, utf8;

static double now_ms(void) {
    struct timespec ts;
    clock_gettime(CLOCK_MONOTONIC, &ts);
    return ts.tv_sec * 1000.0 + ts.tv_nsec / 1e6;
}

static int title_is(Display *d, Window w, const char *title) {
    int ok = 0;
    char *name = NULL;
    if (XFetchName(d, w, &name) && name) {
        ok = strcmp(name, title) == 0;
        XFree(name);
        if (ok) return 1;
    }
    Atom type; int fmt; unsigned long n, after; unsigned char *prop = NULL;
    if (XGetWindowProperty(d, w, net_wm_name, 0, 256, False, utf8, &type, &fmt, &n, &after, &prop) == Success && prop) {
        ok = strcmp((char *)prop, title) == 0;
        XFree(prop);
    }
    return ok;
}

static int find(Display *d, Window w, const char *title, int depth) {
    if (title_is(d, w, title)) {
        XWindowAttributes a;
        if (XGetWindowAttributes(d, w, &a) && a.map_state == IsViewable) return 1;
    }
    if (depth > 4) return 0;
    Window root, parent, *kids = NULL; unsigned int nk = 0;
    if (!XQueryTree(d, w, &root, &parent, &kids, &nk)) return 0;
    int found = 0;
    for (unsigned int i = 0; i < nk && !found; i++) found = find(d, kids[i], title, depth + 1);
    if (kids) XFree(kids);
    return found;
}

int main(int argc, char **argv) {
    if (argc < 4) { fprintf(stderr, "usage: launchwait <title> <timeout_s> <cmd> [args]\n"); return 2; }
    const char *title = argv[1];
    double timeout = atof(argv[2]) * 1000.0;
    Display *d = XOpenDisplay(NULL);
    if (!d) { fprintf(stderr, "cannot open display\n"); return 2; }
    XSetErrorHandler(ignore_err);
    net_wm_name = XInternAtom(d, "_NET_WM_NAME", False);
    utf8 = XInternAtom(d, "UTF8_STRING", False);
    Window root = DefaultRootWindow(d);

    if (find(d, root, title, 0)) { fprintf(stderr, "a window titled %s is already mapped; close it first\n", title); return 3; }
    double t0 = now_ms();
    pid_t pid = fork();
    if (pid == 0) {
        setpgid(0, 0);
        dup2(2, 1); /* keep the caller's stdout pipe free so $(...) returns */
        execvp(argv[3], argv + 3);
        perror("exec");
        _exit(127);
    }
    for (;;) {
        if (find(d, root, title, 0)) {
            printf("%.1f %d\n", now_ms() - t0, pid);
            fflush(stdout);
            return 0;
        }
        int st;
        if (waitpid(pid, &st, WNOHANG) == pid) { fprintf(stderr, "child exited (status %d)\n", st); return 1; }
        if (now_ms() - t0 > timeout) { fprintf(stderr, "timeout\n"); printf("-1 %d\n", pid); return 1; }
        usleep(2000);
    }
}

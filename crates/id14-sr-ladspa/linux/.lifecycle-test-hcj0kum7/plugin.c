
#include <stdlib.h>
struct hint { int descriptor; float lower, upper; };
struct descriptor { unsigned long id; const char *label; int properties;
    const char *name, *maker, *copyright; unsigned long count;
    const int *ports; const char * const *names; const struct hint *hints;
    void *implementation;
    void *(*instantiate)(const struct descriptor *, unsigned long);
    void (*connect)(void *, unsigned long, float *);
    void (*activate)(void *);
    void (*run)(void *, unsigned long);
    void (*run_adding)(void *, unsigned long);
    void (*set_gain)(void *, float);
    void (*deactivate)(void *);
    void (*cleanup)(void *);
};
struct instance { float *ports[5]; };
static void *instantiate(const struct descriptor *d, unsigned long rate) {
    (void)d; (void)rate; return calloc(1, sizeof(struct instance));
}
static void connect(void *handle, unsigned long port, float *data) {
    if (port < 5) ((struct instance *)handle)->ports[port] = data;
}
static void run(void *handle, unsigned long count) {
    struct instance *instance = handle;
    for (unsigned long i = 0; i < count; ++i) {
        instance->ports[2][i] = instance->ports[0][i];
        instance->ports[3][i] = instance->ports[1][i];
    }
}
static const int ports[] = {9, 9, 10, 10, 5};
static const char *names[] = {"Input L", "Input R", "Output L", "Output R", "Mix"};
static const struct hint hints[] = {{0,0,0}, {0,0,0}, {0,0,0}, {0,0,0}, {0x43,-1,200}};
static const struct descriptor descriptor = {1, "id14_sr_stereo", 0,
    "fixture", "fixture", "fixture", 5, ports, names, hints,
    NULL, instantiate, connect, NULL, run, NULL, NULL, NULL, free};
const void *ladspa_descriptor(unsigned long i) { return i ? 0 : &descriptor; }

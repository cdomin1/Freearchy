/*
 * libsensors shim for FreeBSD — answers the lm_sensors calls desktop
 * shells make, backed by sysctl thermal nodes instead of /sys/class/hwmon.
 *
 * Data model: one synthetic chip ("freebsd") whose TEMP features map to
 *   "Package id 0"  →  hw.acpi.thermal.tz0.temperature   (deciKelvin)
 *   "Tctl"          →  average of dev.cpu.N.temperature   (deciKelvin)
 * Labels are picked to match what lm_sensors consumers look for.
 */
#include <sys/types.h>
#include <sys/sysctl.h>

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "sensors/sensors.h"

#define MAX_FEATURES 8

struct feat {
    sensors_feature f;
    sensors_subfeature sf;
    char label[32];
    char sysctl[64]; /* empty when avg_cpu set */
    int avg_cpu;
};

static struct feat g_feats[MAX_FEATURES];
static int g_nfeats;
static sensors_chip_name g_chip;
static char g_chip_prefix[] = "freebsd";
static char g_chip_path[] = "/dev/freebsd-thermal";

static int sysctl_exists(const char *name) {
    size_t len = 0;
    return sysctlbyname(name, NULL, &len, NULL, 0) == 0;
}

static int sysctl_int(const char *name, int *out) {
    int val = 0;
    size_t len = sizeof(val);
    if (sysctlbyname(name, &val, &len, NULL, 0) != 0 || len != sizeof(val))
        return -1;
    *out = val;
    return 0;
}

static void add_feature(const char *name, const char *label,
                        const char *sysctl_name, int avg_cpu) {
    if (g_nfeats >= MAX_FEATURES)
        return;
    struct feat *ft = &g_feats[g_nfeats];
    ft->f.name = name;
    ft->f.number = g_nfeats;
    ft->f.type = SENSORS_FEATURE_TEMP;
    ft->sf.name = "input";
    ft->sf.number = g_nfeats + 1;
    ft->sf.type = SENSORS_SUBFEATURE_TEMP_INPUT;
    ft->sf.mapping = ft->f.number;
    strlcpy(ft->label, label, sizeof(ft->label));
    if (sysctl_name)
        strlcpy(ft->sysctl, sysctl_name, sizeof(ft->sysctl));
    ft->avg_cpu = avg_cpu;
    g_nfeats++;
}

int sensors_init(FILE *input) {
    (void)input;
    g_nfeats = 0;
    if (sysctl_exists("hw.acpi.thermal.tz0.temperature"))
        add_feature("temp1", "Package id 0",
                    "hw.acpi.thermal.tz0.temperature", 0);
    if (sysctl_exists("dev.cpu.0.temperature"))
        add_feature("temp2", "Tctl", NULL, 1);
    g_chip.prefix = g_chip_prefix;
    g_chip.bus.type = SENSORS_BUS_TYPE_ACPI;
    g_chip.bus.nr = 0;
    g_chip.addr = 0;
    g_chip.path = g_chip_path;
    return 0;
}

void sensors_cleanup(void) {}

const sensors_chip_name *sensors_get_detected_chips(
    const sensors_chip_name *match, int *nr) {
    (void)match;
    if (g_nfeats == 0 || *nr >= 1)
        return NULL;
    (*nr)++;
    return &g_chip;
}

const sensors_feature *sensors_get_features(
    const sensors_chip_name *name, int *nr) {
    (void)name;
    if (*nr >= g_nfeats)
        return NULL;
    return &g_feats[(*nr)++].f;
}

const sensors_subfeature *sensors_get_subfeature(
    const sensors_chip_name *name, const sensors_feature *feature,
    sensors_subfeature_type type) {
    (void)name;
    if (type != SENSORS_SUBFEATURE_TEMP_INPUT)
        return NULL;
    for (int i = 0; i < g_nfeats; i++) {
        if (&g_feats[i].f == feature)
            return &g_feats[i].sf;
    }
    return NULL;
}

static double decikelvin_to_celsius(int dk) {
    return dk / 10.0 - 273.15;
}

int sensors_get_value(
    const sensors_chip_name *name, int subfeat_nr, double *value) {
    (void)name;
    for (int i = 0; i < g_nfeats; i++) {
        struct feat *ft = &g_feats[i];
        if (ft->sf.number != subfeat_nr)
            continue;
        if (ft->avg_cpu) {
            int ncpu = 0;
            if (sysctl_int("kern.smp.cpus", &ncpu) != 0 || ncpu <= 0)
                return -1;
            double sum = 0.0;
            int got = 0;
            for (int c = 0; c < ncpu; c++) {
                char node[64];
                int dk;
                snprintf(node, sizeof(node), "dev.cpu.%d.temperature", c);
                if (sysctl_int(node, &dk) == 0) {
                    sum += decikelvin_to_celsius(dk);
                    got++;
                }
            }
            if (!got)
                return -1;
            *value = sum / got;
            return 0;
        }
        int dk;
        if (sysctl_int(ft->sysctl, &dk) != 0)
            return -1;
        *value = decikelvin_to_celsius(dk);
        return 0;
    }
    return -1;
}

char *sensors_get_label(
    const sensors_chip_name *name, const sensors_feature *feature) {
    (void)name;
    for (int i = 0; i < g_nfeats; i++) {
        if (&g_feats[i].f == feature)
            return strdup(g_feats[i].label);
    }
    return NULL;
}

int sensors_snprintf_chip_name(
    char *str, size_t size, const sensors_chip_name *chip) {
    (void)chip;
    return snprintf(str, size, "freebsd-acpi-0000");
}

const char *sensors_strerror(int errnum, char *buf, size_t buflen) {
    if (buf && buflen)
        snprintf(buf, buflen, "libsensors-shim error %d", errnum);
    return buf;
}

int sensors_parse_chip_name(const char *orig_name, sensors_chip_name *res) {
    (void)orig_name;
    (void)res;
    return -1;
}

void sensors_free_chip_name(sensors_chip_name *chip) {
    (void)chip;
}

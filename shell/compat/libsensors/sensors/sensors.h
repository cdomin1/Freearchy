/*
 * Minimal lm_sensors-compatible header for FreeBSD.
 *
 * Implements just the API surface desktop shells actually use
 * (adapter discovery + temp reads). Values match upstream lm_sensors
 * where they matter; consumers that need more should ask before
 * the table grows.
 *
 * ABI note: struct layouts mirror lm_sensors 3.x enough for the
 * calls below; this is NOT a drop-in for software using the full API.
 */
#ifndef FREERACHY_LIBSENSORS_SHIM_H
#define FREERACHY_LIBSENSORS_SHIM_H

#include <limits.h>
#include <stdio.h>

#ifdef __cplusplus
extern "C" {
#endif

#define SENSORS_API_VER 0x440

#define SENSORS_CHIP_NAME_ADDR_ANY    -1
#define SENSORS_BUS_TYPE_ANY          -1
#define SENSORS_BUS_TYPE_I2C          0
#define SENSORS_BUS_TYPE_ISA          1
#define SENSORS_BUS_TYPE_PCI          2
#define SENSORS_BUS_TYPE_SPI          3
#define SENSORS_BUS_TYPE_VIRTUAL      4
#define SENSORS_BUS_TYPE_ACPI         5
#define SENSORS_BUS_TYPE_HID          6
#define SENSORS_BUS_NR_ANY            (-1)
#define SENSORS_BUS_NR_IGNORE         (-2)

typedef enum sensors_feature_type {
    SENSORS_FEATURE_IN          = 0x00,
    SENSORS_FEATURE_FAN         = 0x01,
    SENSORS_FEATURE_TEMP        = 0x02,
    SENSORS_FEATURE_POWER       = 0x03,
    SENSORS_FEATURE_ENERGY      = 0x05,
    SENSORS_FEATURE_CURR        = 0x06,
    SENSORS_FEATURE_HUMIDITY    = 0x07,
    SENSORS_FEATURE_INTRUSION   = 0x0B,
    SENSORS_FEATURE_UNKNOWN     = INT_MAX,
} sensors_feature_type;

#define SENSORS_SUBFEATURE_TEMP_INPUT  (SENSORS_FEATURE_TEMP << 8 | 0x00)
#define SENSORS_SUBFEATURE_TEMP_MAX    (SENSORS_FEATURE_TEMP << 8 | 0x01)

typedef int sensors_subfeature_type;

typedef struct sensors_bus_id {
    short type;
    short nr;
} sensors_bus_id;

typedef struct sensors_chip_name {
    char *prefix;
    sensors_bus_id bus;
    int addr;
    char *path;
} sensors_chip_name;

typedef struct sensors_feature {
    const char *name;
    int number;
    sensors_feature_type type;
    int first_subfeature;
    int padding1;
} sensors_feature;

typedef struct sensors_subfeature {
    const char *name;
    int number;
    sensors_subfeature_type type;
    int mapping;
    unsigned int flags;
} sensors_subfeature;

int sensors_init(FILE *input);
void sensors_cleanup(void);

const sensors_chip_name *sensors_get_detected_chips(
    const sensors_chip_name *match, int *nr);
const sensors_feature *sensors_get_features(
    const sensors_chip_name *name, int *nr);
const sensors_subfeature *sensors_get_subfeature(
    const sensors_chip_name *name, const sensors_feature *feature,
    sensors_subfeature_type type);
int sensors_get_value(
    const sensors_chip_name *name, int subfeat_nr, double *value);
char *sensors_get_label(
    const sensors_chip_name *name, const sensors_feature *feature);
int sensors_snprintf_chip_name(
    char *str, size_t size, const sensors_chip_name *chip);
const char *sensors_strerror(int errnum, char *buf, size_t buflen);
int sensors_parse_chip_name(const char *orig_name, sensors_chip_name *res);
void sensors_free_chip_name(sensors_chip_name *chip);

#ifdef __cplusplus
}
#endif

#endif /* FREERACHY_LIBSENSORS_SHIM_H */

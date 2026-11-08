#ifndef PURR_CORE_H
#define PURR_CORE_H

#include <stdbool.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

bool purr_core_disclaimer_accepted(void);
int32_t purr_core_accept_disclaimer(const char* typed);
int32_t purr_core_init(void);
void purr_core_shutdown(void);
bool purr_core_is_awake(void);
void purr_core_set_awake(bool awake);
void purr_core_set_mode(const char* mode);
int32_t purr_core_set_tun(bool enabled);
bool purr_core_tun_authorized(void);
int32_t purr_core_restart_core(void);
int32_t purr_core_select_node(const char* group, const char* node);
char* purr_core_test_group(const char* group);
char* purr_core_probe_sites_json(void);
char* purr_core_import_subscription(const char* url);
char* purr_core_use_subscription(uint64_t id);
char* purr_core_update_subscription(uint64_t id);
int32_t purr_core_close_all_connections(void);
int32_t purr_core_close_connection(const char* id);
char* purr_core_snapshot_json(void);
char* purr_core_check_update_json(void);
/// {"ok":bool,"message":str}; action "swap" or "retry" acts on the trouble the core detected.
char* purr_core_fix_trouble_json(const char* action);
char* purr_core_load_preferences_json(void);
int32_t purr_core_update_preference(const char* key, const char* value);
int32_t purr_core_set_system_proxy(bool enabled);
int32_t purr_core_flush_dns(void);
char* purr_core_version(void);
void purr_string_free(char* ptr);

#ifdef __cplusplus
}
#endif

#endif

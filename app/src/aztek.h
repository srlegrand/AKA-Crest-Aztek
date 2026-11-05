// The Rust host library's C interface. Strings are UTF-8; returned strings are freed with aztek_free.
#pragma once
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

void aztek_free(char* p);
void aztek_init(void);
char* aztek_mel(const char* src);
int aztek_python(const char* src);
char* aztek_exec(const char* name, const char* call_json);
char* aztek_exec_quiet(const char* name, const char* call_json);
void aztek_retract(void);
char* aztek_ask(const char* name, const char* call_json);
char* aztek_take_log(void);
void aztek_log(const char* text, int kind);
uint64_t aztek_version(void);
char* aztek_query(const char* what, const char* arg);
char* aztek_file(const char* op, const char* path);
void aztek_undo(void);
void aztek_redo(void);

enum { AZTEK_XCB = 1, AZTEK_XLIB = 2, AZTEK_WIN32 = 3, AZTEK_WAYLAND = 4 };

int aztek_view_create(int platform, void* display, uint64_t window, void* extra, int width, int height, double dpr,
                      const char* camera);
void aztek_view_destroy(int id);
void aztek_view_resize(int id, int width, int height);
void aztek_view_render(int id);
int aztek_view_mouse(int id, int kind, int button, double x, double y, int mods, int buttons);
void aztek_view_wheel(int id, double delta);
void aztek_view_set(int id, const char* key, const char* value_json);
char* aztek_view_get(int id, const char* key);
int aztek_view_capture(int id, const char* path);
char* aztek_view_hit(int id, double x, double y);

#ifdef __cplusplus
}
#endif

#ifndef CALUMMA_H
#define CALUMMA_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct CalmEngine CalmEngine;

typedef enum CalmStatus {
    CalmStatusOk = 0,
    CalmStatusError = 1,
    CalmStatusNull = 2,
} CalmStatus;

typedef enum CalmOpKind {
    CalmOpKindRemoveBackground = 0,
    CalmOpKindGenerateTexture = 1,
    CalmOpKindVectorize = 2,
    CalmOpKindSuggestShape = 3,
} CalmOpKind;

typedef enum CalmOpOutputKind {
    CalmOpOutputKindNone = 0,
    CalmOpOutputKindMask = 1,
    CalmOpOutputKindRaster = 2,
} CalmOpOutputKind;

typedef struct CalmOpInput {
    const uint8_t *rgba;
    uint32_t w;
    uint32_t h;
} CalmOpInput;

typedef struct CalmOpOutput {
    CalmOpOutputKind kind;
    uint8_t *data;
    size_t len;
    uint32_t w;
    uint32_t h;
} CalmOpOutput;

typedef bool (*CalmOpAvailableFn)(CalmOpKind kind);
typedef CalmStatus (*CalmOpRunFn)(CalmOpKind kind, const CalmOpInput *input, CalmOpOutput *out);
typedef void (*CalmOpFreeFn)(CalmOpOutput *out);

typedef struct CalmPlatformOps {
    CalmOpAvailableFn available;
    CalmOpRunFn run;
    CalmOpFreeFn free_output;
} CalmPlatformOps;

typedef struct CalmState {
    uint32_t width;
    uint32_t height;
    float zoom;
    float min_zoom;
    float max_zoom;
    float pan_x;
    float pan_y;
    uint32_t active_layer;
    uint32_t layer_count;
    uint8_t can_undo;
    uint8_t can_redo;
    uint8_t stroke_active;
    uint8_t dark_theme;
} CalmState;

typedef struct CalmProjectInfo {
    char *id;
    char *name;
    uint32_t width;
    uint32_t height;
    int64_t opened_at;
} CalmProjectInfo;

void calm_string_free(char *s);
void calm_buffer_free(uint8_t *ptr, size_t len);

CalmEngine *calm_engine_new(const char *db_path);
void calm_engine_free(CalmEngine *engine);

CalmStatus calm_engine_attach_surface(CalmEngine *engine, void *metal_layer, uint32_t w, uint32_t h, float scale);
CalmStatus calm_engine_resize(CalmEngine *engine, uint32_t w, uint32_t h, float scale);
CalmStatus calm_engine_render(CalmEngine *engine);

CalmStatus calm_engine_pointer_down(CalmEngine *engine, float x, float y);
CalmStatus calm_engine_pointer_move(CalmEngine *engine, float x, float y);
CalmStatus calm_engine_pointer_up(CalmEngine *engine, float x, float y);
CalmStatus calm_engine_pan(CalmEngine *engine, float dx, float dy);
CalmStatus calm_engine_zoom(CalmEngine *engine, float x, float y, float factor);
CalmStatus calm_engine_fit(CalmEngine *engine);
CalmStatus calm_engine_set_zoom(CalmEngine *engine, float zoom);

CalmStatus calm_engine_set_tool(CalmEngine *engine, uint32_t tool);
CalmStatus calm_engine_set_color(CalmEngine *engine, uint8_t r, uint8_t g, uint8_t b, uint8_t a);
CalmStatus calm_engine_set_brush(CalmEngine *engine, float size);
CalmStatus calm_engine_set_fill(CalmEngine *engine, uint8_t fill);
CalmStatus calm_engine_set_dark(CalmEngine *engine, uint8_t dark);

CalmStatus calm_engine_undo(CalmEngine *engine);
CalmStatus calm_engine_redo(CalmEngine *engine);
CalmStatus calm_engine_add_layer(CalmEngine *engine);
CalmStatus calm_engine_remove_layer(CalmEngine *engine, uint32_t index);
CalmStatus calm_engine_set_layer_visible(CalmEngine *engine, uint32_t index, uint8_t visible);
int calm_engine_layer_visible(CalmEngine *engine, uint32_t index);
CalmStatus calm_engine_set_active_layer(CalmEngine *engine, uint32_t index);
CalmStatus calm_engine_set_hover_layer(CalmEngine *engine, int32_t index);
CalmStatus calm_engine_clear_layer(CalmEngine *engine);
CalmStatus calm_engine_state(CalmEngine *engine, CalmState *out);
char *calm_engine_layer_name(CalmEngine *engine, uint32_t index);
CalmStatus calm_engine_layer_thumbnail(CalmEngine *engine, uint32_t layer_index, uint32_t max_side, uint8_t **out_rgba, uint32_t *out_w, uint32_t *out_h);

char *calm_project_create(CalmEngine *engine, const char *name, uint32_t width, uint32_t height);
CalmStatus calm_project_open(CalmEngine *engine, const char *id);
CalmStatus calm_project_close(CalmEngine *engine);
size_t calm_project_list(CalmEngine *engine, CalmProjectInfo *out, size_t cap);
CalmStatus calm_project_delete(CalmEngine *engine, const char *id);
CalmStatus calm_project_save(CalmEngine *engine);

CalmStatus calm_engine_install_platform_ops(CalmEngine *engine, const CalmPlatformOps *ops);
bool calm_engine_op_available(CalmEngine *engine, uint32_t kind);
CalmStatus calm_engine_run_op(CalmEngine *engine, uint32_t kind, uint32_t layer_index);

#ifdef __cplusplus
}
#endif

#endif

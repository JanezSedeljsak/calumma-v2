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
    uint32_t accent;
    float zoom_unit;
} CalmState;

typedef struct CalmMemory {
    uint64_t tile_bytes;
    uint64_t history_bytes;
    uint64_t mask_bytes;
    uint64_t vector_bytes;
    uint64_t text_bytes;
    uint64_t gpu_bytes;
    uint32_t tile_count;
    uint32_t shared_tile_count;
} CalmMemory;

typedef struct CalmAdjustments {
    float brightness;
    float contrast;
    float vibrance;
    float saturation;
    float levels_gamma;
} CalmAdjustments;

typedef struct CalmProjectInfo {
    char *id;
    char *name;
    uint32_t width;
    uint32_t height;
    int64_t opened_at;
    uint32_t accent;
} CalmProjectInfo;

typedef struct CalmWorkspaceInfo {
    char *id;
    char *name;
    uint32_t accent;
    char *active_project_id;
    int64_t opened_at;
} CalmWorkspaceInfo;

void calm_string_free(char *s);
void calm_buffer_free(uint8_t *ptr, size_t len);

CalmEngine *calm_engine_new(const char *db_path);
void calm_engine_free(CalmEngine *engine);

CalmStatus calm_engine_attach_surface(CalmEngine *engine, void *metal_layer, uint32_t w, uint32_t h, float scale);
CalmStatus calm_engine_resize(CalmEngine *engine, uint32_t w, uint32_t h, float scale);
CalmStatus calm_engine_resize_document(CalmEngine *engine, uint32_t width, uint32_t height);
CalmStatus calm_engine_render(CalmEngine *engine);

CalmStatus calm_engine_pointer_down(CalmEngine *engine, float x, float y);
CalmStatus calm_engine_pointer_move(CalmEngine *engine, float x, float y);
CalmStatus calm_engine_pointer_up(CalmEngine *engine, float x, float y);
CalmStatus calm_engine_pan(CalmEngine *engine, float dx, float dy);

CalmStatus calm_engine_pan_scroll(CalmEngine *engine, float dx, float dy, uint8_t precise);
CalmStatus calm_engine_zoom(CalmEngine *engine, float x, float y, float factor);
CalmStatus calm_engine_zoom_scroll(CalmEngine *engine, float x, float y, float delta,
                                   uint8_t precise);
CalmStatus calm_engine_fit(CalmEngine *engine);
CalmStatus calm_engine_set_zoom(CalmEngine *engine, float zoom);
CalmStatus calm_engine_step_zoom(CalmEngine *engine, uint8_t zoom_in);
CalmStatus calm_engine_set_zoom_unit(CalmEngine *engine, float unit);
CalmStatus calm_engine_set_board_colors(CalmEngine *engine, uint32_t desk, uint32_t grid, uint32_t paper_border);

uint32_t calm_palette_count(void);
uint32_t calm_palette_color(uint32_t index);
CalmStatus calm_project_rename(CalmEngine *engine, const char *id, const char *name);
CalmStatus calm_project_set_accent(CalmEngine *engine, const char *id, uint32_t accent);

CalmStatus calm_engine_set_tool(CalmEngine *engine, uint32_t tool);
CalmStatus calm_engine_set_color(CalmEngine *engine, uint8_t r, uint8_t g, uint8_t b, uint8_t a);
CalmStatus calm_engine_sample_color(CalmEngine *engine, float x, float y, uint32_t *out_rgba);
CalmStatus calm_engine_pick_color(CalmEngine *engine, float x, float y, uint32_t *out_rgba);
CalmStatus calm_engine_set_brush(CalmEngine *engine, float size);
CalmStatus calm_engine_set_fill(CalmEngine *engine, uint8_t fill);
CalmStatus calm_engine_set_dark(CalmEngine *engine, uint8_t dark);
CalmStatus calm_engine_set_shift(CalmEngine *engine, uint8_t held);
CalmStatus calm_engine_reset_layer_transform(CalmEngine *engine, uint32_t index);
CalmStatus calm_engine_toggle_transform(CalmEngine *engine);
CalmStatus calm_engine_exit_transform(CalmEngine *engine);

CalmStatus calm_engine_undo(CalmEngine *engine);
CalmStatus calm_engine_redo(CalmEngine *engine);
CalmStatus calm_engine_add_layer(CalmEngine *engine);
CalmStatus calm_engine_remove_layer(CalmEngine *engine, uint32_t index);
CalmStatus calm_engine_set_layer_visible(CalmEngine *engine, uint32_t index, uint8_t visible);
int calm_engine_layer_visible(CalmEngine *engine, uint32_t index);
CalmStatus calm_engine_set_active_layer(CalmEngine *engine, uint32_t index);
CalmStatus calm_engine_duplicate_layer(CalmEngine *engine, uint32_t index);
CalmStatus calm_engine_merge_layer_down(CalmEngine *engine, uint32_t index);
CalmStatus calm_engine_set_layer_opacity(CalmEngine *engine, uint32_t index, float opacity);
float calm_engine_layer_opacity(CalmEngine *engine, uint32_t index);
CalmStatus calm_engine_set_layer_blend_mode(CalmEngine *engine, uint32_t index, uint32_t mode);
uint32_t calm_engine_layer_blend_mode(CalmEngine *engine, uint32_t index);
CalmStatus calm_engine_set_layer_adjustments(CalmEngine *engine, uint32_t index, float brightness, float contrast, float vibrance, float saturation, float levels_gamma);
CalmStatus calm_engine_nudge_layer_adjustment(CalmEngine *engine, uint32_t index, uint32_t kind, float steps);
CalmStatus calm_engine_layer_adjustments(CalmEngine *engine, uint32_t index, CalmAdjustments *out);
CalmStatus calm_engine_set_hover_layer(CalmEngine *engine, int32_t index);
CalmStatus calm_engine_clear_layer(CalmEngine *engine);
CalmStatus calm_engine_state(CalmEngine *engine, CalmState *out);
CalmStatus calm_engine_memory(CalmEngine *engine, CalmMemory *out);
char *calm_engine_layer_name(CalmEngine *engine, uint32_t index);
CalmStatus calm_engine_layer_thumbnail(CalmEngine *engine, uint32_t layer_index, uint32_t max_side, uint8_t **out_rgba, uint32_t *out_w, uint32_t *out_h);
CalmStatus calm_engine_composite_rgba(CalmEngine *engine, uint8_t **out_rgba, uint32_t *out_w, uint32_t *out_h);
CalmStatus calm_engine_export_psd(CalmEngine *engine, uint8_t **out_bytes, size_t *out_len);
CalmStatus calm_engine_layer_rgba(CalmEngine *engine, uint32_t layer_index, uint8_t **out_rgba, uint32_t *out_w, uint32_t *out_h);
char *calm_engine_layer_svg(CalmEngine *engine, uint32_t layer_index);
char *calm_engine_export_svg(CalmEngine *engine);
CalmStatus calm_engine_selection_rgba(CalmEngine *engine, uint8_t **out_rgba, uint32_t *out_w, uint32_t *out_h);
int calm_engine_has_selection(CalmEngine *engine);
CalmStatus calm_engine_deselect(CalmEngine *engine);
CalmStatus calm_engine_selection_clear_pixels(CalmEngine *engine);
CalmStatus calm_engine_paste_image(CalmEngine *engine, const uint8_t *premultiplied_rgba, size_t len, uint32_t width, uint32_t height);

typedef enum CalmCaretStep {
    CalmCaretStepLeft = 0,
    CalmCaretStepRight = 1,
    CalmCaretStepUp = 2,
    CalmCaretStepDown = 3,
    CalmCaretStepLineStart = 4,
    CalmCaretStepLineEnd = 5,
    CalmCaretStepDocStart = 6,
    CalmCaretStepDocEnd = 7,
} CalmCaretStep;

typedef enum CalmTextAlign {
    CalmTextAlignLeft = 0,
    CalmTextAlignCenter = 1,
    CalmTextAlignRight = 2,
} CalmTextAlign;

typedef enum CalmFontStyle {
    CalmFontStyleBold = 1,
    CalmFontStyleItalic = 2,
} CalmFontStyle;

uint32_t calm_font_family_count(void);
char *calm_font_family_name(uint32_t index);
uint32_t calm_font_family_styles(uint32_t index);
float calm_text_size_min(void);
float calm_text_size_max(void);
float calm_text_size_default(void);
float calm_text_line_height_min(void);
float calm_text_line_height_max(void);
float calm_text_line_height_default(void);
CalmStatus calm_engine_text_insert(CalmEngine *engine, const char *text);
CalmStatus calm_engine_text_set_marked(CalmEngine *engine, const char *text);
CalmStatus calm_engine_text_backspace(CalmEngine *engine);
CalmStatus calm_engine_text_delete_forward(CalmEngine *engine);
CalmStatus calm_engine_text_move_caret(CalmEngine *engine, uint32_t step);
CalmStatus calm_engine_text_commit(CalmEngine *engine);
CalmStatus calm_engine_text_edit_layer(CalmEngine *engine, uint32_t index);
int calm_engine_text_editing(CalmEngine *engine);
int calm_engine_layer_is_text(CalmEngine *engine, uint32_t index);
CalmStatus calm_engine_set_text_family(CalmEngine *engine, const char *family);
CalmStatus calm_engine_set_text_size(CalmEngine *engine, float size);
CalmStatus calm_engine_set_text_align(CalmEngine *engine, uint32_t align);
CalmStatus calm_engine_set_text_bold(CalmEngine *engine, int bold);
CalmStatus calm_engine_set_text_italic(CalmEngine *engine, int italic);
CalmStatus calm_engine_set_text_line_height(CalmEngine *engine, float line_height);
CalmStatus calm_engine_rasterize_text_layer(CalmEngine *engine, uint32_t index);
char *calm_engine_text_family(CalmEngine *engine);
float calm_engine_text_size(CalmEngine *engine);
uint32_t calm_engine_text_align(CalmEngine *engine);
float calm_engine_text_line_height(CalmEngine *engine);
uint32_t calm_engine_text_styles(CalmEngine *engine);
CalmStatus calm_engine_text_caret_rect(CalmEngine *engine, float *out_x, float *out_y, float *out_height);
char *calm_engine_layer_text(CalmEngine *engine, uint32_t index);

CalmStatus calm_engine_set_vector_mode(CalmEngine *engine, uint8_t on);
int calm_engine_vector_mode(CalmEngine *engine);
int calm_engine_layer_is_vector(CalmEngine *engine, uint32_t index);
uint32_t calm_engine_layer_item_count(CalmEngine *engine, uint32_t index);
int calm_engine_selected_vector_item(CalmEngine *engine);
CalmStatus calm_engine_clear_vector_selection(CalmEngine *engine);
CalmStatus calm_engine_delete_selected_vector_item(CalmEngine *engine);
CalmStatus calm_engine_nudge_selected_vector_item(CalmEngine *engine, float steps_x, float steps_y);

char *calm_project_create(CalmEngine *engine, const char *name, uint32_t width, uint32_t height);
uint32_t calm_import_max_side(void);
char *calm_project_create_from_image(CalmEngine *engine, const char *name, uint32_t width, uint32_t height, const uint8_t *premultiplied_rgba, size_t len);
CalmStatus calm_project_open(CalmEngine *engine, const char *id);
CalmStatus calm_project_close(CalmEngine *engine);
size_t calm_project_list(CalmEngine *engine, CalmProjectInfo *out, size_t cap);
CalmStatus calm_project_delete(CalmEngine *engine, const char *id);
CalmStatus calm_project_save(CalmEngine *engine);
CalmStatus calm_project_thumbnail(CalmEngine *engine, const char *project_id, uint8_t **out_png, size_t *out_len);

size_t calm_workspace_list(CalmEngine *engine, CalmWorkspaceInfo *out, size_t cap);
char *calm_workspace_create(CalmEngine *engine, const char *name);
CalmStatus calm_workspace_rename(CalmEngine *engine, const char *id, const char *name);
CalmStatus calm_workspace_set_accent(CalmEngine *engine, const char *id, uint32_t accent);
CalmStatus calm_workspace_delete(CalmEngine *engine, const char *id);
CalmStatus calm_workspace_add_project(CalmEngine *engine, const char *workspace_id, const char *project_id);
CalmStatus calm_workspace_remove_project(CalmEngine *engine, const char *workspace_id, const char *project_id);
size_t calm_workspace_projects(CalmEngine *engine, const char *workspace_id, CalmProjectInfo *out, size_t cap);
CalmStatus calm_workspace_set_active_project(CalmEngine *engine, const char *workspace_id, const char *project_id);
CalmStatus calm_workspace_touch(CalmEngine *engine, const char *id);
CalmStatus calm_workspace_get(CalmEngine *engine, const char *id, CalmWorkspaceInfo *out);
char *calm_workspace_create_for_project(CalmEngine *engine, const char *project_id, const char *name);
char *calm_workspace_for_project(CalmEngine *engine, const char *project_id);
size_t calm_open_workspace_tabs(CalmEngine *engine, char **out, size_t cap);
CalmStatus calm_set_open_workspace_tabs(CalmEngine *engine, const char *const *ids, size_t count);

CalmStatus calm_engine_install_platform_ops(CalmEngine *engine, const CalmPlatformOps *ops);
bool calm_engine_op_available(CalmEngine *engine, uint32_t kind);
CalmStatus calm_engine_run_op(CalmEngine *engine, uint32_t kind, uint32_t layer_index);

#ifdef __cplusplus
}
#endif

#endif

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
    uint32_t last_shape_tool;
    uint32_t last_select_tool;
    uint8_t is_fit;
    uint8_t transform_active;
} CalmState;

typedef struct CalmRulerTick {
    float doc;
    uint8_t major;
} CalmRulerTick;

typedef struct CalmMemory {
    uint64_t tile_bytes;
    uint64_t history_bytes;
    uint64_t mask_bytes;
    uint64_t vector_bytes;
    uint64_t text_bytes;
    uint64_t preview_bytes;
    uint64_t gpu_bytes;
    uint32_t tile_count;
    uint32_t shared_tile_count;
} CalmMemory;

typedef struct CalmLayerBounds {
    float x;
    float y;
    float width;
    float height;
} CalmLayerBounds;

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
CalmStatus calm_fit_size(float viewport_width, float viewport_height, float doc_width,
                         float doc_height, float *out_width, float *out_height);

typedef struct CalmDeskMetrics {
    float cell;
    float line_width;
    float cross_arm;
    float cross_line_width;
    float line_alpha;
} CalmDeskMetrics;

CalmStatus calm_desk_metrics(CalmDeskMetrics *out);
CalmStatus calm_fit_camera(float viewport_width, float viewport_height, float doc_width,
                           float doc_height, float *out_zoom, float *out_pan_x, float *out_pan_y);
CalmStatus calm_engine_viewport(CalmEngine *engine, float *out_width, float *out_height);
CalmStatus calm_engine_end_camera_motion(CalmEngine *engine);
CalmStatus calm_engine_set_zoom(CalmEngine *engine, float zoom);
CalmStatus calm_engine_step_zoom(CalmEngine *engine, uint8_t zoom_in);
CalmStatus calm_engine_set_zoom_unit(CalmEngine *engine, float unit);
CalmStatus calm_engine_set_board_colors(CalmEngine *engine, uint32_t desk, uint32_t grid, uint32_t paper_border);

uint32_t calm_palette_count(void);
uint32_t calm_palette_color(uint32_t index);
CalmStatus calm_project_rename(CalmEngine *engine, const char *id, const char *name);
CalmStatus calm_project_set_accent(CalmEngine *engine, const char *id, uint32_t accent);

CalmStatus calm_engine_set_tool(CalmEngine *engine, uint32_t tool);
uint8_t calm_tool_is_shape(uint32_t tool);
uint8_t calm_tool_is_selection(uint32_t tool);
uint8_t calm_tool_takes_fill(uint32_t tool);
uint8_t calm_tool_takes_brush_size(uint32_t tool);
uint8_t calm_tool_takes_ink_opacity(uint32_t tool);
uint8_t calm_tool_shows_vector_mode(uint32_t tool);
uint8_t calm_tool_takes_blur_strength(uint32_t tool);
uint8_t calm_tool_takes_tolerance(uint32_t tool);
uint8_t calm_tool_takes_eyedropper_radius(uint32_t tool);
uint8_t calm_tool_takes_brush(uint32_t tool);
uint8_t calm_tool_takes_eraser_hardness(uint32_t tool);

// Why a tool cannot run on the active layer: 0 none, 1 locked, 2 text layer, 3 vector layer,
// 4 nothing to work on. `calm_engine_tool_blocks` fills `out[tool]` for every tool below
// `len` and returns how many it wrote.
uint32_t calm_engine_tool_blocks(CalmEngine *engine, uint32_t *out, uint32_t len);
uint32_t calm_engine_tool_block(CalmEngine *engine, uint32_t tool);
CalmStatus calm_engine_take_tool_block_notice(CalmEngine *engine, uint32_t *out);
int calm_engine_vector_mode_locked(CalmEngine *engine);
int calm_engine_layer_is_rasterizable(CalmEngine *engine, uint32_t index);
CalmStatus calm_engine_rasterize_layer(CalmEngine *engine, uint32_t index);
CalmStatus calm_parse_hex_rgb(const char *s, uint32_t *out_rgb);
char *calm_format_hex_rgb(uint32_t rgb);
float calm_lossy_export_quality(void);
float calm_pdf_default_dpi(void);
// Brush size: range, slider curve (0..1 travel <-> size), and one `[` / `]` press.
float calm_brush_size_min(void);
float calm_brush_size_max(void);
float calm_brush_size_default(void);
float calm_brush_size_unit(float size);
float calm_brush_size_from_unit(float unit);
float calm_brush_size_step(float size, uint8_t increase);
float calm_ink_opacity_min(void);
float calm_ink_opacity_max(void);
float calm_ink_opacity_default(void);
float calm_blur_strength_min(void);
float calm_blur_strength_max(void);
float calm_blur_strength_default(void);
float calm_eraser_hardness_min(void);
float calm_eraser_hardness_max(void);
float calm_eraser_hardness_default(void);
uint8_t calm_tolerance_min(void);
uint8_t calm_tolerance_max(void);
uint8_t calm_tolerance_default(void);
uint32_t calm_eyedropper_radius_min(void);
uint32_t calm_eyedropper_radius_max(void);
uint32_t calm_eyedropper_radius_default(void);
CalmStatus calm_engine_set_color(CalmEngine *engine, uint8_t r, uint8_t g, uint8_t b, uint8_t a);
CalmStatus calm_engine_sample_color(CalmEngine *engine, float x, float y, uint32_t *out_rgba);
CalmStatus calm_engine_pick_color(CalmEngine *engine, float x, float y, uint32_t *out_rgba);
CalmStatus calm_engine_set_brush(CalmEngine *engine, float size);
CalmStatus calm_engine_set_ink_opacity(CalmEngine *engine, float opacity);
CalmStatus calm_engine_set_blur_strength(CalmEngine *engine, float strength);
CalmStatus calm_engine_set_tolerance(CalmEngine *engine, uint8_t tolerance);
CalmStatus calm_engine_set_eyedropper_radius(CalmEngine *engine, uint32_t radius);
CalmStatus calm_engine_set_brush_kind(CalmEngine *engine, uint32_t brush);
CalmStatus calm_engine_set_eraser_hardness(CalmEngine *engine, float hardness);
CalmStatus calm_engine_set_fill(CalmEngine *engine, uint8_t fill);
CalmStatus calm_engine_set_stroke(CalmEngine *engine, uint8_t stroke);
CalmStatus calm_engine_set_stroke_color(CalmEngine *engine, uint8_t r, uint8_t g, uint8_t b, uint8_t a);
CalmStatus calm_engine_set_shape_fill_color(CalmEngine *engine, uint8_t r, uint8_t g, uint8_t b, uint8_t a);
CalmStatus calm_engine_set_dark(CalmEngine *engine, uint8_t dark);
CalmStatus calm_engine_set_shift(CalmEngine *engine, uint8_t held);
CalmStatus calm_engine_reset_layer_transform(CalmEngine *engine, uint32_t index);
CalmStatus calm_engine_set_pointer_hover(CalmEngine *engine, float x, float y);
CalmStatus calm_engine_clear_pointer_hover(CalmEngine *engine);
int calm_engine_brush_ring_visible(CalmEngine *engine);
CalmStatus calm_engine_toggle_transform(CalmEngine *engine);
CalmStatus calm_engine_enter_transform(CalmEngine *engine);
CalmStatus calm_engine_exit_transform(CalmEngine *engine);

CalmStatus calm_engine_undo(CalmEngine *engine);
CalmStatus calm_engine_redo(CalmEngine *engine);
CalmStatus calm_engine_add_layer(CalmEngine *engine);
CalmStatus calm_engine_remove_layer(CalmEngine *engine, uint32_t index);
CalmStatus calm_engine_set_layer_visible(CalmEngine *engine, uint32_t index, uint8_t visible);
int calm_engine_layer_visible(CalmEngine *engine, uint32_t index);
CalmStatus calm_engine_set_active_layer(CalmEngine *engine, uint32_t index);
CalmStatus calm_engine_duplicate_layer(CalmEngine *engine, uint32_t index);
CalmStatus calm_engine_move_layer_up(CalmEngine *engine, uint32_t index);
CalmStatus calm_engine_move_layer_down(CalmEngine *engine, uint32_t index);
CalmStatus calm_engine_move_layer_row(CalmEngine *engine, uint32_t from_row, uint32_t to_row);
CalmStatus calm_engine_set_layer_name(CalmEngine *engine, uint32_t index, const char *name);
CalmStatus calm_engine_set_layer_locked(CalmEngine *engine, uint32_t index, uint8_t locked);
int calm_engine_layer_locked(CalmEngine *engine, uint32_t index);
int calm_engine_layer_is_paper(CalmEngine *engine, uint32_t index);
CalmStatus calm_engine_merge_layer_down(CalmEngine *engine, uint32_t index);
CalmStatus calm_engine_clip_layer_down(CalmEngine *engine, uint32_t index);
int calm_engine_layer_can_clip_down(CalmEngine *engine, uint32_t index);
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
size_t calm_ruler_ticks_x(float zoom, float pan, float viewport_extent, CalmRulerTick *out,
                          size_t cap);
size_t calm_ruler_ticks_y(float zoom, float pan, float viewport_extent, CalmRulerTick *out,
                          size_t cap);
size_t calm_engine_ruler_ticks_x(CalmEngine *engine, CalmRulerTick *out, size_t cap);
size_t calm_engine_ruler_ticks_y(CalmEngine *engine, CalmRulerTick *out, size_t cap);
CalmStatus calm_engine_guide_drag_from_ruler(CalmEngine *engine, uint8_t axis, float x, float y);
CalmStatus calm_engine_guide_drag_update(CalmEngine *engine, float x, float y);
CalmStatus calm_engine_guide_drag_end(CalmEngine *engine, float x, float y);
CalmStatus calm_engine_clear_guides(CalmEngine *engine);
size_t calm_engine_guide_count(CalmEngine *engine);
int calm_engine_guide_axis_at(CalmEngine *engine, float x, float y);
int calm_engine_dragged_guide(CalmEngine *engine, uint8_t *out_axis, float *out_position,
                              float *out_screen);

typedef struct CalmGuide {
    uint8_t axis;
    float position;
    // Packed 0xRRGGBB, the same shape calm_palette_color hands back.
    uint32_t color;
} CalmGuide;

size_t calm_engine_guide_list(CalmEngine *engine, CalmGuide *out, size_t cap);
CalmStatus calm_engine_add_guide(CalmEngine *engine, uint8_t axis, float position);
CalmStatus calm_engine_set_guide_position(CalmEngine *engine, size_t index, float position);
CalmStatus calm_engine_set_guide_axis(CalmEngine *engine, size_t index, uint8_t axis);
// rgb is packed 0xRRGGBB. A guide has no alpha to set — how solid a rule is drawn is what says
// whether it is the one being dragged.
CalmStatus calm_engine_set_guide_color(CalmEngine *engine, size_t index, uint32_t rgb);
uint32_t calm_default_guide_color(void);
CalmStatus calm_engine_remove_guide(CalmEngine *engine, size_t index);
size_t calm_guides_limit(void);
// Frames per second the engine wants from here, or 0 for "as fast as the display allows".
// The ceiling stays the shell's — this is only the floor the engine can live with.
uint32_t calm_engine_frame_hint(CalmEngine *engine);
CalmStatus calm_engine_memory(CalmEngine *engine, CalmMemory *out);
// level: 0 = normal, 1 = warn, 2 = critical — mirrors DISPATCH_SOURCE_TYPE_MEMORYPRESSURE.
CalmStatus calm_engine_set_memory_pressure(CalmEngine *engine, uint32_t level);
char *calm_engine_layer_name(CalmEngine *engine, uint32_t index);
CalmStatus calm_engine_layer_thumbnail(CalmEngine *engine, uint32_t layer_index, uint32_t max_side, uint8_t **out_rgba, uint32_t *out_w, uint32_t *out_h);
char *calm_engine_layer_id(CalmEngine *engine, uint32_t index);
uint64_t calm_engine_layer_preview_revision(CalmEngine *engine, uint32_t index);
CalmStatus calm_engine_layer_bounds(CalmEngine *engine, uint32_t index, CalmLayerBounds *out);
CalmStatus calm_engine_set_layer_bounds(CalmEngine *engine, uint32_t index, float x, float y, float width, float height);
CalmStatus calm_engine_composite_rgba(CalmEngine *engine, uint8_t **out_rgba, uint32_t *out_w, uint32_t *out_h);
CalmStatus calm_engine_export_psd(CalmEngine *engine, uint8_t **out_bytes, size_t *out_len);
CalmStatus calm_engine_export_pdf(CalmEngine *engine, float dpi, uint8_t **out_bytes,
                                  size_t *out_len);
CalmStatus calm_engine_layer_rgba(CalmEngine *engine, uint32_t layer_index, uint8_t **out_rgba, uint32_t *out_w, uint32_t *out_h);
char *calm_engine_layer_svg(CalmEngine *engine, uint32_t layer_index);
char *calm_engine_export_svg(CalmEngine *engine);
CalmStatus calm_engine_selection_rgba(CalmEngine *engine, uint8_t **out_rgba, uint32_t *out_w, uint32_t *out_h);
int calm_engine_has_selection(CalmEngine *engine);
CalmStatus calm_engine_copy(CalmEngine *engine, uint8_t **out, size_t *out_len, uint32_t *out_kind);
CalmStatus calm_engine_cut(CalmEngine *engine, uint8_t **out, size_t *out_len, uint32_t *out_kind);
CalmStatus calm_engine_copy_layer(CalmEngine *engine, uint32_t layer_index, uint8_t **out, size_t *out_len, uint32_t *out_kind);
CalmStatus calm_engine_deselect(CalmEngine *engine);
CalmStatus calm_engine_select_all(CalmEngine *engine);
CalmStatus calm_engine_invert_selection(CalmEngine *engine);
CalmStatus calm_engine_selection_clear_pixels(CalmEngine *engine);
CalmStatus calm_engine_paste_image(CalmEngine *engine, const uint8_t *premultiplied_rgba, size_t len, uint32_t width, uint32_t height, uint32_t *out_outcome);

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
float calm_text_size_unit(float size);
float calm_text_size_from_unit(float unit);
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
int calm_engine_nudge_move_target(CalmEngine *engine, float steps_x, float steps_y);

char *calm_project_create(CalmEngine *engine, const char *name, uint32_t width, uint32_t height);
uint32_t calm_import_max_side(void);
char *calm_project_create_from_image(CalmEngine *engine, const char *name, uint32_t width, uint32_t height, const uint8_t *premultiplied_rgba, size_t len);
CalmStatus calm_project_open(CalmEngine *engine, const char *id);
CalmStatus calm_project_close(CalmEngine *engine);
size_t calm_project_list(CalmEngine *engine, CalmProjectInfo *out, size_t cap);
CalmStatus calm_project_get(CalmEngine *engine, const char *id, CalmProjectInfo *out);
size_t calm_open_project_tabs(CalmEngine *engine, char **out, size_t cap);
CalmStatus calm_set_open_project_tabs(CalmEngine *engine, const char *const *ids, size_t count);
CalmStatus calm_project_delete(CalmEngine *engine, const char *id);
CalmStatus calm_project_delete_all(CalmEngine *engine);
CalmStatus calm_project_save(CalmEngine *engine);
CalmStatus calm_project_thumbnail(CalmEngine *engine, const char *project_id, uint8_t **out_png, size_t *out_len);

CalmStatus calm_engine_install_platform_ops(CalmEngine *engine, const CalmPlatformOps *ops);
bool calm_engine_op_available(CalmEngine *engine, uint32_t kind);
CalmStatus calm_engine_run_op(CalmEngine *engine, uint32_t kind, uint32_t layer_index);

#ifdef __cplusplus
}
#endif

#endif

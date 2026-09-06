#pragma once

#include <Calumma.hpp>

#include <cstdint>
#include <memory>
#include <string>
#include <utility>
#include <vector>

namespace calumma {

// Every engine resource is owned by a deleter that calls the one function the C ABI provides
// for it. Nothing in the shell frees engine memory by hand, and no raw CalmEngine * escapes
// this header.
struct EngineDeleter {
    void operator()(CalmEngine *engine) const noexcept { calm_engine_free(engine); }
};

struct StringDeleter {
    void operator()(char *s) const noexcept { calm_string_free(s); }
};

using EnginePtr = std::unique_ptr<CalmEngine, EngineDeleter>;
using EngineString = std::unique_ptr<char, StringDeleter>;

// An engine-allocated buffer. `calm_buffer_free` wants the length back along with the pointer,
// which is why this is a small class rather than a unique_ptr with a deleter — and why it is
// moved rather than copied: a composite or a PSD is megabytes, and nothing here needs two.
class Bytes {
public:
    Bytes() = default;
    Bytes(uint8_t *data, size_t size) : m_data(data), m_size(size) {}
    ~Bytes() { reset(); }

    Bytes(const Bytes &) = delete;
    Bytes &operator=(const Bytes &) = delete;
    Bytes(Bytes &&other) noexcept : m_data(other.m_data), m_size(other.m_size) {
        other.m_data = nullptr;
        other.m_size = 0;
    }
    Bytes &operator=(Bytes &&other) noexcept {
        if (this != &other) {
            reset();
            m_data = other.m_data;
            m_size = other.m_size;
            other.m_data = nullptr;
            other.m_size = 0;
        }
        return *this;
    }

    const uint8_t *data() const noexcept { return m_data; }
    size_t size() const noexcept { return m_size; }
    bool empty() const noexcept { return m_data == nullptr || m_size == 0; }

private:
    void reset() noexcept {
        if (m_data != nullptr) {
            calm_buffer_free(m_data, m_size);
            m_data = nullptr;
            m_size = 0;
        }
    }

    uint8_t *m_data = nullptr;
    size_t m_size = 0;
};

struct Rgba {
    uint8_t r = 0;
    uint8_t g = 0;
    uint8_t b = 0;
    uint8_t a = 255;
};

struct Image {
    Bytes rgba;
    uint32_t width = 0;
    uint32_t height = 0;

    bool valid() const noexcept { return !rgba.empty() && width > 0 && height > 0; }
};

struct ProjectInfo {
    std::string id;
    std::string name;
    uint32_t width = 0;
    uint32_t height = 0;
    int64_t openedAt = 0;
    uint32_t accent = 0;
};

struct Viewport {
    float width = 0.0f;
    float height = 0.0f;
};

struct Guide {
    uint8_t axis = 0;
    float position = 0.0f;
    uint32_t color = 0;
};

struct DraggedGuide {
    uint8_t axis = 0;
    float position = 0.0f;
    float screen = 0.0f;
};

struct CaretRect {
    float x = 0.0f;
    float y = 0.0f;
    float height = 0.0f;
};

struct Bounds {
    float x = 0.0f;
    float y = 0.0f;
    float width = 0.0f;
    float height = 0.0f;
};

// What came off the clipboard, in the engine's terms: the bytes plus which of the shell's
// pasteboard types they should be offered as.
struct Clipping {
    Bytes bytes;
    uint32_t kind = 0;
};

// Several files dropped or pasted at once. The bytes are borrowed for the length of the call
// only — the engine copies what it keeps — so these hold pointers rather than buffers.
struct EncodedImage {
    std::string name;
    const uint8_t *bytes = nullptr;
    size_t len = 0;
};

struct PasteImage {
    std::string name;
    const uint8_t *premultipliedRgba = nullptr;
    size_t len = 0;
    uint32_t width = 0;
    uint32_t height = 0;
};

// The C++ half of what Engine.swift does: one method per FFI call, CalmStatus checked here so
// callers deal in bool, std::string and small structs rather than status codes and owned
// pointers. Deliberately free of Qt — this is the layer a headless test can drive, and phase
// 5's QML knobs become a QObject that owns one of these rather than a second copy of it.
//
// Method definitions are split by concern across Engine*.cpp, the same split the Swift bridge
// makes with extensions. Anything without a CalmEngine * — limits, defaults, per-tool
// predicates — is not a method at all; it lives in Limits.hpp.
class Engine {
public:
    // dbPath empty means the engine picks the OS-native app-data directory itself, which is
    // the only behaviour the shell should ever want.
    explicit Engine(const std::string &dbPath = {});

    Engine(const Engine &) = delete;
    Engine &operator=(const Engine &) = delete;

    bool isValid() const noexcept { return m_engine != nullptr; }
    CalmEngine *raw() const noexcept { return m_engine.get(); }

    // Surface and frame
    bool attach(const CalmNativeSurface &surface, uint32_t width, uint32_t height, float scale);
    bool resize(uint32_t width, uint32_t height, float scale);
    bool resizeDocument(uint32_t width, uint32_t height);
    bool render();
    uint32_t frameHint() const;
    CalmState state() const;
    Viewport viewport() const;

    // Camera. Fit and zoom limits are the engine's; the shell never recomputes them.
    bool fit();
    bool setZoom(float zoom);
    bool stepZoom(bool zoomIn);
    bool setZoomUnit(float unit);
    bool endCameraMotion();

    // History
    bool undo();
    bool redo();

    // Board appearance, pushed from the shell's tokens.
    bool setBoardColors(uint32_t desk, uint32_t grid, uint32_t paperBorder);
    bool setDark(bool dark);
    bool setMemoryPressure(uint32_t level);
    CalmMemory memory() const;

    // --- EngineProjects.cpp ----------------------------------------------------------------
    std::string createProject(const std::string &name, uint32_t width, uint32_t height);
    std::string createProjectFromImage(const std::string &name, uint32_t width, uint32_t height,
                                       const uint8_t *premultipliedRgba, size_t len);
    std::string createProjectFromEncoded(const std::string &name, const uint8_t *bytes, size_t len);
    std::string createProjectFromImages(const std::string &name,
                                        const std::vector<PasteImage> &images);
    std::string createProjectFromEncodedImages(const std::string &name,
                                               const std::vector<EncodedImage> &images);
    bool openProject(const std::string &id);
    bool closeProject();
    bool saveProject();
    bool deleteProject(const std::string &id);
    bool deleteAllProjects();
    bool renameProject(const std::string &id, const std::string &name);
    bool setProjectAccent(const std::string &id, uint32_t accent);
    bool project(const std::string &id, ProjectInfo &out) const;
    std::vector<ProjectInfo> projects(size_t limit = 32) const;
    Bytes projectThumbnail(const std::string &id) const;
    std::vector<std::string> openTabs(size_t limit = 32) const;
    bool setOpenTabs(const std::vector<std::string> &ids);

    // --- EngineInput.cpp -------------------------------------------------------------------
    bool pointerDown(float x, float y);
    bool pointerMove(float x, float y);
    bool pointerUp(float x, float y);
    bool pan(float dx, float dy);
    bool panScroll(float dx, float dy, bool precise);
    bool zoom(float x, float y, float factor);
    bool zoomScroll(float x, float y, float delta, bool precise);
    bool setShift(bool held);
    bool setAlt(bool held);
    bool setPointerHover(float x, float y);
    bool clearPointerHover();
    bool brushRingVisible() const;
    bool setHoverLayer(int32_t index);
    bool nudgeMoveTarget(float stepsX, float stepsY);

    // --- EngineTools.cpp -------------------------------------------------------------------
    bool setTool(uint32_t tool);
    bool setColor(Rgba color);
    bool setStrokeColor(Rgba color);
    bool setShapeFillColor(Rgba color);
    bool setSelectColor(Rgba color);
    bool selectColor(uint32_t &outRgba) const;
    bool setBrush(float size);
    bool setBrushKind(uint32_t brush);
    bool setInkOpacity(float opacity);
    bool setEraserHardness(float hardness);
    bool setBlurStrength(float strength);
    bool setTolerance(uint8_t tolerance);
    bool setEyedropperRadius(uint32_t radius);
    bool setCloneAligned(bool aligned);
    bool setFill(bool fill);
    bool setStroke(bool stroke);
    bool sampleColor(float x, float y, uint32_t &outRgba) const;
    bool pickColor(float x, float y, uint32_t &outRgba) const;

    // --- EngineToolGate.cpp ----------------------------------------------------------------
    // Why a tool cannot run on the active layer. The shell greys buttons by asking; it never
    // works the reason out for itself.
    uint32_t toolBlock(uint32_t tool) const;
    std::vector<uint32_t> toolBlocks(uint32_t toolCount) const;
    bool takeToolBlockNotice(uint32_t &outTool);
    bool vectorModeLocked() const;
    bool layerIsRasterizable(uint32_t index) const;
    bool rasterizeLayer(uint32_t index);

    // --- EngineLayers.cpp ------------------------------------------------------------------
    bool addLayer();
    bool removeLayer(uint32_t index);
    bool duplicateLayer(uint32_t index);
    bool mergeLayerDown(uint32_t index);
    bool clipLayerDown(uint32_t index);
    bool layerCanClipDown(uint32_t index) const;
    bool moveLayerUp(uint32_t index);
    bool moveLayerDown(uint32_t index);
    bool moveLayerRow(uint32_t fromRow, uint32_t toRow);
    bool clearLayer();
    bool setActiveLayer(uint32_t index);
    bool setLayerSelection(const std::vector<uint32_t> &indices);
    bool alignLayers(const std::vector<uint32_t> &indices, uint32_t edge);
    bool distributeLayers(const std::vector<uint32_t> &indices, uint32_t axis);
    std::string layerName(uint32_t index) const;
    std::string layerId(uint32_t index) const;
    bool setLayerName(uint32_t index, const std::string &name);
    bool layerVisible(uint32_t index) const;
    bool setLayerVisible(uint32_t index, bool visible);
    bool layerLocked(uint32_t index) const;
    bool setLayerLocked(uint32_t index, bool locked);
    bool layerIsPaper(uint32_t index) const;
    bool layerIsText(uint32_t index) const;
    bool layerIsVector(uint32_t index) const;
    uint32_t layerItemCount(uint32_t index) const;
    float layerOpacity(uint32_t index) const;
    bool setLayerOpacity(uint32_t index, float opacity);
    uint32_t layerBlendMode(uint32_t index) const;
    bool setLayerBlendMode(uint32_t index, uint32_t mode);
    CalmAdjustments layerAdjustments(uint32_t index) const;
    bool setLayerAdjustments(uint32_t index, const CalmAdjustments &adjustments);
    bool nudgeLayerAdjustment(uint32_t index, uint32_t kind, float steps);
    bool layerBounds(uint32_t index, Bounds &out) const;
    bool setLayerBounds(uint32_t index, const Bounds &bounds);
    bool resetLayerTransform(uint32_t index);
    uint64_t layerPreviewRevision(uint32_t index) const;

    // --- EngineTransform.cpp ---------------------------------------------------------------
    bool toggleTransform();
    bool enterTransform();
    bool exitTransform();
    bool setCropAspectLock(float ratio);
    bool clearCropAspectLock();
    bool setCropOverlayStyle(uint32_t style);
    bool setStraightenActive(bool active);
    bool commitCrop();

    // --- EngineClipboard.cpp ---------------------------------------------------------------
    bool hasSelection() const;
    bool selectAll();
    bool invertSelection();
    bool deselect();
    bool selectionClearPixels();
    Image selectionRgba() const;
    Clipping copy();
    Clipping cut();
    Clipping copyLayer(uint32_t layerIndex);
    bool pasteImage(const uint8_t *premultipliedRgba, size_t len, uint32_t width, uint32_t height,
                    uint32_t &outOutcome);
    bool pasteEncoded(const uint8_t *bytes, size_t len, uint32_t &outOutcome);
    bool pasteImages(const std::vector<PasteImage> &images, uint32_t &outCount,
                     uint32_t &outOutcome);
    bool pasteEncodedImages(const std::vector<EncodedImage> &images, uint32_t &outCount,
                            uint32_t &outOutcome);

    // --- EngineGuides.cpp ------------------------------------------------------------------
    std::vector<CalmRulerTick> rulerTicksX(size_t cap = 256) const;
    std::vector<CalmRulerTick> rulerTicksY(size_t cap = 256) const;
    bool guideDragFromRuler(uint8_t axis, float x, float y);
    bool guideDragUpdate(float x, float y);
    bool guideDragEnd(float x, float y);
    bool clearGuides();
    size_t guideCount() const;
    // -1 when the point is not on a guide; otherwise the axis.
    int guideAxisAt(float x, float y) const;
    bool draggedGuide(DraggedGuide &out) const;
    std::vector<Guide> guides(size_t cap = 64) const;
    bool addGuide(uint8_t axis, float position);
    bool setGuidePosition(size_t index, float position);
    bool setGuideAxis(size_t index, uint8_t axis);
    bool setGuideColor(size_t index, uint32_t rgb);
    bool removeGuide(size_t index);

    // --- EngineText.cpp --------------------------------------------------------------------
    bool textInsert(const std::string &text);
    bool textSetMarked(const std::string &text);
    bool textBackspace();
    bool textDeleteForward();
    bool textMoveCaret(uint32_t step, bool extend);
    bool textSelectAll();
    bool textSelectWordAt(float x, float y);
    bool textSelectParagraphAt(float x, float y);
    bool textHasSelection() const;
    bool textCommit();
    bool textEditLayer(uint32_t index);
    bool textEditing() const;
    bool textCaretRect(CaretRect &out) const;
    std::string layerText(uint32_t index) const;
    bool setTextFamily(const std::string &family);
    bool setTextSize(float size);
    bool setTextAlign(uint32_t align);
    bool setTextBold(bool bold);
    bool setTextItalic(bool italic);
    bool setTextLineHeight(float lineHeight);
    bool setTextWrapWidth(float width);
    std::string textFamily() const;
    float textSize() const;
    uint32_t textAlign() const;
    float textLineHeight() const;
    uint32_t textStyles() const;
    float textWrapWidth() const;
    float textWrapMax() const;

    // --- EngineVector.cpp ------------------------------------------------------------------
    bool setVectorMode(bool on);
    bool vectorMode() const;
    // -1 when nothing is selected.
    int selectedVectorItem() const;
    bool clearVectorSelection();
    bool deleteSelectedVectorItem();
    bool nudgeSelectedVectorItem(float stepsX, float stepsY);

    // --- EngineExport.cpp ------------------------------------------------------------------
    Bytes exportImage(uint32_t format) const;
    Bytes exportLayerImage(uint32_t layerIndex, uint32_t format) const;
    Bytes exportPsd() const;
    Bytes exportPdf(float dpi) const;
    std::string exportSvg() const;
    std::string layerSvg(uint32_t layerIndex) const;
    Image compositeRgba() const;
    Image layerRgba(uint32_t layerIndex) const;
    Image layerThumbnail(uint32_t layerIndex, uint32_t maxSide) const;

    // --- EngineOps.cpp ---------------------------------------------------------------------
    // Remove Background and the rest of the platform ops. Windows and Linux install no vtable,
    // so `opAvailable` is false there and the shell greys the entry rather than hiding it.
    bool installPlatformOps(const CalmPlatformOps &ops);
    bool opAvailable(uint32_t kind) const;
    bool runOp(uint32_t kind, uint32_t layerIndex);
    bool upscaleLayer(uint32_t layerIndex, float scale);
    bool smartMatte(uint32_t layerIndex);
    bool seamCarveLayer(uint32_t layerIndex, uint32_t width, uint32_t height);

private:
    // Two shapes cover nearly every FFI call: one that reports a CalmStatus, and one that
    // answers a value. Both are a no-op on an engine that failed to open, so no method below
    // repeats the null check that would otherwise be its first line.
    template <typename Fn>
    bool call(Fn &&fn) const {
        return m_engine != nullptr && std::forward<Fn>(fn)(m_engine.get()) == CalmStatusOk;
    }

    template <typename Fn, typename T>
    T query(Fn &&fn, T fallback) const {
        return m_engine != nullptr ? std::forward<Fn>(fn)(m_engine.get()) : fallback;
    }

    // Owned strings from the C ABI, turned into std::string and freed. An empty string is what
    // a caller gets for "no answer", which is the same thing every one of these means.
    std::string ownedString(char *s) const;

    // The `uint8_t **out, size_t *out_len` pair every buffer-returning call uses.
    template <typename Fn>
    Bytes ownedBytes(Fn &&fn) const {
        uint8_t *data = nullptr;
        size_t len = 0;
        if (!call([&](CalmEngine *e) { return std::forward<Fn>(fn)(e, &data, &len); })) {
            return {};
        }
        return Bytes(data, len);
    }

    // The same, for the calls that answer pixels and their size.
    template <typename Fn>
    Image ownedImage(Fn &&fn) const {
        uint8_t *data = nullptr;
        uint32_t width = 0;
        uint32_t height = 0;
        if (!call([&](CalmEngine *e) { return std::forward<Fn>(fn)(e, &data, &width, &height); })) {
            return {};
        }
        Image image;
        image.rgba = Bytes(data, static_cast<size_t>(width) * height * 4);
        image.width = width;
        image.height = height;
        return image;
    }

    EnginePtr m_engine;
};

}  // namespace calumma

import XCTest

final class EngineSmokeTests: XCTestCase {
    func testEngineNewAndFree() {
        let engine = calm_engine_new(nil)
        XCTAssertNotNil(engine)
        calm_engine_free(engine)
    }

    func testEngineStateAfterCreate() {
        let engine = calm_engine_new(nil)
        XCTAssertNotNil(engine)
        defer { calm_engine_free(engine) }

        let name = "Smoke"
        let id = name.withCString { calm_project_create(engine, $0, 64, 64) }
        XCTAssertNotNil(id)
        calm_string_free(id)

        var state = CalmState()
        let status = calm_engine_state(engine, &state)
        XCTAssertEqual(status, CalmStatusOk)
        XCTAssertEqual(state.width, 64)
        XCTAssertEqual(state.height, 64)
        XCTAssertEqual(state.layer_count, 1)
    }
}

class_name GdTestCompatGdUnit
extends RefCounted

const GDUNIT4_CHAIN := preload("res://addons/gdtest/gdunit4_chain.gd")

# GdTestCompatGdUnit — compatibility base class for tests written in GDUnit4 style.
#
# Migration from GDUnit4: replace `extends GdUnitTestSuite` with
# `extends GdTestCompatGdUnit`. Lifecycle hooks and assertions map as follows:
#
#   GDUnit4               → gdtest
#   before() / after()    → suite-level (before_all / after_all aliases)
#   before_test()         → before_each alias
#   after_test()          → after_each alias
#   assert_str(v)…        → typed fluent asserters returning _GdUnit4Chain
#   assert_signal(node)… → GdtSignalAssert (fluent, 1-arg — different from GdTest.assert_signal)
#   auto_free(obj)        → Gdt.autoref(obj)
#
# Not supported (see Linear tickets):
#   @GdUnitMock, @GdUnitTestParameters, fuzz_*

func _h() -> Object:
	return Engine.get_meta("gdt_harness") if Engine.has_meta("gdt_harness") else null

# ── Typed asserters (GDUnit4 primary API) ─────────────────────────────────────

func assert_that(value: Variant) -> Object:
	return GDUNIT4_CHAIN.new(value, _h())

func assert_bool(value: bool) -> Object:
	return GDUNIT4_CHAIN.new(value, _h())

func assert_int(value: int) -> Object:
	return GDUNIT4_CHAIN.new(value, _h())

func assert_float(value: float) -> Object:
	return GDUNIT4_CHAIN.new(value, _h())

func assert_str(value: String) -> Object:
	return GDUNIT4_CHAIN.new(value, _h())

func assert_array(value: Array) -> Object:
	return GDUNIT4_CHAIN.new(value, _h())

func assert_dict(value: Dictionary) -> Object:
	return GDUNIT4_CHAIN.new(value, _h())

func assert_object(value: Object) -> Object:
	return GDUNIT4_CHAIN.new(value, _h())

func assert_vector2(value: Vector2) -> Object:
	return GDUNIT4_CHAIN.new(value, _h())

func assert_vector3(value: Vector3) -> Object:
	return GDUNIT4_CHAIN.new(value, _h())

# Signal asserter — fluent GDUnit4 style (1 arg, returns GdtSignalAssert).
# This differs from GdTest.assert_signal which takes (node, sig, timeout_ms).
func assert_signal(node: Object) -> Object:
	return Gdt.assert_signal_of(node)

# ── Flat assertions (GUT-compatible names, also available in GDUnit4) ─────────

func assert_eq(actual: Variant, expected: Variant, msg: String = "") -> bool:
	return Gdt.expect_eq(actual, expected, msg)

func assert_ne(actual: Variant, not_expected: Variant, msg: String = "") -> bool:
	return Gdt.expect_ne(actual, not_expected, msg)

func assert_true(cond: bool, msg: String = "") -> bool:
	return Gdt.expect_true(cond, msg)

func assert_false(cond: bool, msg: String = "") -> bool:
	return Gdt.expect_false(cond, msg)

func assert_null(value: Variant, msg: String = "") -> bool:
	return Gdt.expect_null(value, msg)

func assert_not_null(value: Variant, msg: String = "") -> bool:
	return Gdt.expect_not_null(value, msg)

func assert_gt(actual: Variant, bound: Variant, msg: String = "") -> bool:
	return Gdt.expect_gt(actual, bound, msg)

func assert_gte(actual: Variant, bound: Variant, msg: String = "") -> bool:
	return Gdt.expect_gte(actual, bound, msg)

func assert_lt(actual: Variant, bound: Variant, msg: String = "") -> bool:
	return Gdt.expect_lt(actual, bound, msg)

func assert_lte(actual: Variant, bound: Variant, msg: String = "") -> bool:
	return Gdt.expect_lte(actual, bound, msg)

func assert_almost_eq(actual: float, expected: float, tolerance: float = 1e-6, msg: String = "") -> bool:
	return Gdt.expect_approx(actual, expected, tolerance, msg)

func assert_between(value: Variant, low: Variant, high: Variant, msg: String = "") -> bool:
	return Gdt.assert_between(value, low, high, msg)

func assert_has(collection: Variant, value: Variant, msg: String = "") -> bool:
	return Gdt.assert_has(collection, value, msg)

func assert_not_has(collection: Variant, value: Variant, msg: String = "") -> bool:
	return Gdt.assert_not_has(collection, value, msg)

func assert_is(obj: Variant, klass: Variant, msg: String = "") -> bool:
	return Gdt.assert_is(obj, klass, msg)

func assert_file(path: String) -> Object:
	return Gdt.assert_file(path)

func fail(msg: String = "explicit fail") -> void:
	Gdt.fail(msg)

func pending(msg: String = "not yet implemented") -> void:
	Gdt.pending(msg)

func skip(msg: String = "") -> void:
	Gdt.skip(msg)

# ── Utilities ─────────────────────────────────────────────────────────────────

# GDUnit4 equivalent of add_autofree / autoref.
func auto_free(obj: Object) -> Object:
	return Gdt.autoref(obj)

func add_child_autofree(node: Node) -> Node:
	return Gdt.add_child_autofree(node)

func instantiate_scene(packed: PackedScene) -> Node:
	return Gdt.instantiate_scene(packed)

func create_temp_dir(name: String) -> String:
	return Gdt.create_temp_dir(name)

func delete_temp_dir(name: String) -> void:
	Gdt.delete_temp_dir(name)

func scene_runner(packed: PackedScene) -> SceneRunner:
	return SceneRunner.new(instantiate_scene(packed))

# ── Async helpers ─────────────────────────────────────────────────────────────

func wait_physics_frames(n: int = 1) -> void:
	await Gdt.wait_physics_frames(n)

func wait_physics_seconds(seconds: float) -> void:
	await Gdt.wait_physics_seconds(seconds)

func wait_process_frame() -> void:
	await Gdt.wait_process_frame()

func wait_until(predicate: Callable, timeout_ms: float = 2000.0) -> void:
	await Gdt.wait_until(predicate, timeout_ms)

# ── Signal assertions (GUT-compatible style, also available here) ─────────────

func assert_signal_count(node: Object, sig: String, expected_count: int, timeout_ms: float = 1000.0) -> void:
	await Gdt.assert_signal_count(node, sig, expected_count, timeout_ms)

func assert_signal_with_args(node: Object, sig: String, expected_args: Array, timeout_ms: float = 1000.0) -> void:
	await Gdt.assert_signal_with_args(node, sig, expected_args, timeout_ms)

func assert_no_signal(node: Object, sig: String, timeout_ms: float = 200.0) -> void:
	await Gdt.assert_no_signal(node, sig, timeout_ms)

func watch_signal(obj: Object, signal_name: String) -> Object:
	return Gdt.watch_signal(obj, signal_name)

# ── Mocking / doubles ─────────────────────────────────────────────────────────

func mock_singleton(name: String) -> Object:
	return Gdt.mock_singleton(name)

func mock(klass: GDScript, auto_free_obj: bool = true) -> Object:
	return Gdt.mock(klass, auto_free_obj)

func spy(real: Object, auto_free_obj: bool = true) -> Object:
	return Gdt.spy(real, auto_free_obj)

func partial_mock(klass: GDScript, auto_free_obj: bool = true) -> Object:
	return Gdt.partial_mock(klass, auto_free_obj)

func stub(mock_obj: Object, method_name: String, value: Variant = null) -> void:
	Gdt.stub(mock_obj, method_name, value)

func stub_with_args(mock_obj: Object, method_name: String, args: Array, value: Variant = null) -> void:
	Gdt.stub_with_args(mock_obj, method_name, args, value)

func get_calls(mock_obj: Object, method_name: String) -> Array:
	return Gdt.get_calls(mock_obj, method_name)

func assert_called(mock_obj: Object, method_name: String, msg: String = "") -> bool:
	return Gdt.assert_called(mock_obj, method_name, msg)

func assert_not_called(mock_obj: Object, method_name: String, msg: String = "") -> bool:
	return Gdt.assert_not_called(mock_obj, method_name, msg)

func assert_call_count(mock_obj: Object, method_name: String, expected: int, msg: String = "") -> bool:
	return Gdt.assert_call_count(mock_obj, method_name, expected, msg)

func assert_called_with(mock_obj: Object, method_name: String, args: Array, msg: String = "") -> bool:
	return Gdt.assert_called_with(mock_obj, method_name, args, msg)

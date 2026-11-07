class_name GdTest
extends RefCounted

# GdTest — optional base class for gdtest test files.
#
# Provides the same assertion API as GUT (`assert_eq`, `assert_true`, …) so that
# migration from GUT is a single-line change: `extends GutTest` → `extends GdTest`.
# All assertion calls are identical; tree/async helpers are also compatible.
#
# ── Equality / comparison ─────────────────────────────────────────────────────

func assert_eq(actual: Variant, expected: Variant, msg: String = "") -> bool:
	return Gdt.expect_eq(actual, expected, msg)

func assert_ne(actual: Variant, not_expected: Variant, msg: String = "") -> bool:
	return Gdt.expect_ne(actual, not_expected, msg)

func assert_lt(actual: Variant, bound: Variant, msg: String = "") -> bool:
	return Gdt.expect_lt(actual, bound, msg)

func assert_lte(actual: Variant, bound: Variant, msg: String = "") -> bool:
	return Gdt.expect_lte(actual, bound, msg)

func assert_gt(actual: Variant, bound: Variant, msg: String = "") -> bool:
	return Gdt.expect_gt(actual, bound, msg)

func assert_gte(actual: Variant, bound: Variant, msg: String = "") -> bool:
	return Gdt.expect_gte(actual, bound, msg)

func assert_almost_eq(actual: float, expected: float, tolerance: float = 1e-6, msg: String = "") -> bool:
	return Gdt.expect_approx(actual, expected, tolerance, msg)

func assert_approx(actual: float, expected: float, tolerance: float = 1e-6, msg: String = "") -> bool:
	return Gdt.expect_approx(actual, expected, tolerance, msg)

# ── Boolean / null ────────────────────────────────────────────────────────────

func assert_true(cond: bool, msg: String = "") -> bool:
	return Gdt.expect_true(cond, msg)

func assert_false(cond: bool, msg: String = "") -> bool:
	return Gdt.expect_false(cond, msg)

func assert_null(value: Variant, msg: String = "") -> bool:
	return Gdt.expect_null(value, msg)

func assert_not_null(value: Variant, msg: String = "") -> bool:
	return Gdt.expect_not_null(value, msg)

func assert_all(checks: Callable) -> bool:
	return Gdt.assert_all(checks)

func fail(msg: String = "explicit fail") -> void:
	Gdt.fail(msg)

func pending(msg: String = "not yet implemented") -> void:
	Gdt.pending(msg)

func skip(msg: String = "") -> void:
	Gdt.skip(msg)

# ── Scene tree helpers ────────────────────────────────────────────────────────

func add_child_autofree(node: Node) -> Node:
	return Gdt.add_child_autofree(node)

func instantiate_scene(packed: PackedScene) -> Node:
	return Gdt.instantiate_scene(packed)

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

func wait_while(predicate: Callable, timeout_ms: float = 2000.0) -> void:
	await Gdt.wait_while(predicate, timeout_ms)

func set_physics_delta(seconds: float) -> void:
	Gdt.set_physics_delta(seconds)

func reset_physics_delta() -> void:
	Gdt.reset_physics_delta()

# ── Collection / type / range / string ───────────────────────────────────

func assert_has(collection: Variant, value: Variant, msg: String = "") -> bool:
	return Gdt.assert_has(collection, value, msg)

func assert_not_has(collection: Variant, value: Variant, msg: String = "") -> bool:
	return Gdt.assert_not_has(collection, value, msg)

func assert_is(obj: Variant, klass: Variant, msg: String = "") -> bool:
	return Gdt.assert_is(obj, klass, msg)

func assert_between(value: Variant, low: Variant, high: Variant, msg: String = "") -> bool:
	return Gdt.assert_between(value, low, high, msg)

func assert_string_contains(haystack: String, needle: String, msg: String = "") -> bool:
	return Gdt.assert_string_contains(haystack, needle, msg)

func assert_string_starts_with(s: String, prefix: String, msg: String = "") -> bool:
	return Gdt.assert_string_starts_with(s, prefix, msg)

func assert_string_ends_with(s: String, suffix: String, msg: String = "") -> bool:
	return Gdt.assert_string_ends_with(s, suffix, msg)

func assert_file(path: String) -> Object:
	return Gdt.assert_file(path)

func create_temp_dir(name: String) -> String:
	return Gdt.create_temp_dir(name)

func delete_temp_dir(name: String) -> void:
	Gdt.delete_temp_dir(name)

# ── Godot spatial types ───────────────────────────────────────────────────

func assert_vec2_approx(a: Vector2, b: Vector2, tolerance: float = 1e-5, msg: String = "") -> bool:
	return Gdt.assert_vec2_approx(a, b, tolerance, msg)

func assert_vec3_approx(a: Vector3, b: Vector3, tolerance: float = 1e-5, msg: String = "") -> bool:
	return Gdt.assert_vec3_approx(a, b, tolerance, msg)

func assert_vec4_approx(a: Vector4, b: Vector4, tolerance: float = 1e-5, msg: String = "") -> bool:
	return Gdt.assert_vec4_approx(a, b, tolerance, msg)

func assert_color_approx(a: Color, b: Color, tolerance: float = 1e-5, msg: String = "") -> bool:
	return Gdt.assert_color_approx(a, b, tolerance, msg)

func assert_rect2_approx(a: Rect2, b: Rect2, tolerance: float = 1e-5, msg: String = "") -> bool:
	return Gdt.assert_rect2_approx(a, b, tolerance, msg)

# ── Signal assertions ─────────────────────────────────────────────────────

func assert_signal_count(node: Object, sig: String, expected_count: int, timeout_ms: float = 1000.0) -> void:
	await Gdt.assert_signal_count(node, sig, expected_count, timeout_ms)

func assert_signal_with_args(node: Object, sig: String, expected_args: Array, timeout_ms: float = 1000.0) -> void:
	await Gdt.assert_signal_with_args(node, sig, expected_args, timeout_ms)

func assert_signal(node: Object, sig: String, timeout_ms: float = 1000.0, context_fn: Callable = Callable()) -> void:
	await Gdt.assert_signal(node, sig, timeout_ms, context_fn)

func assert_no_signal(node: Object, sig: String, timeout_ms: float = 200.0) -> void:
	await Gdt.assert_no_signal(node, sig, timeout_ms)

# ── Snapshot assertions ────────────────────────────────────────────

func assert_snapshot(actual: Variant, name: String = "") -> bool:
	return Gdt.assert_snapshot(actual, name, get_script().resource_path)

# ── Mocking / doubles ─────────────────────────────────────────────────────

func mock_singleton(name: String) -> Object:
	return Gdt.mock_singleton(name)

func spy(real: Object, auto_free: bool = true) -> Object:
	return Gdt.spy(real, auto_free)

func partial_mock(klass: GDScript, auto_free: bool = true) -> Object:
	return Gdt.partial_mock(klass, auto_free)

func mock(klass: GDScript, auto_free: bool = true) -> Object:
	return Gdt.mock(klass, auto_free)

func stub(mock_obj: Object, method_name: String, value: Variant = null) -> void:
	Gdt.stub(mock_obj, method_name, value)

func stub_with_args(mock_obj: Object, method_name: String, args: Array, value: Variant = null) -> void:
	Gdt.stub_with_args(mock_obj, method_name, args, value)

func get_calls(mock_obj: Object, method_name: String) -> Array:
	return Gdt.get_calls(mock_obj, method_name)

func get_call_log(mock_obj: Object) -> Array:
	return Gdt.get_call_log(mock_obj)

func assert_called(mock_obj: Object, method_name: String, msg: String = "") -> bool:
	return Gdt.assert_called(mock_obj, method_name, msg)

func assert_not_called(mock_obj: Object, method_name: String, msg: String = "") -> bool:
	return Gdt.assert_not_called(mock_obj, method_name, msg)

func assert_call_count(mock_obj: Object, method_name: String, expected: int, msg: String = "") -> bool:
	return Gdt.assert_call_count(mock_obj, method_name, expected, msg)

func assert_called_with(mock_obj: Object, method_name: String, args: Array, msg: String = "") -> bool:
	return Gdt.assert_called_with(mock_obj, method_name, args, msg)

func assert_called_in_order(mock_obj: Object, method_names: Array, msg: String = "") -> bool:
	return Gdt.assert_called_in_order(mock_obj, method_names, msg)

func assert_no_new_orphans(msg: String = "") -> bool:
	return Gdt.assert_no_new_orphans(msg)

# ── Push error / warning capture ────────────────────────────────────

# ── Sandbox / scenario testing ─────────────────────────────────────

func test_world() -> Object:
	return Gdt.test_world()

# ── Memory tracking + autoref scoping ──────────────────────────────

func autoref(obj: Object) -> Object:
	return Gdt.autoref(obj)

func current_heap_delta() -> int:
	return Gdt.current_heap_delta()

# ── Property / export assertions ────────────────────────────────────

func assert_exports(obj: Object, prop_name: String, type: int, msg: String = "") -> bool:
	return Gdt.assert_exports(obj, prop_name, type, msg)

func assert_property(obj: Object, prop_name: String, default_value: Variant, msg: String = "") -> bool:
	return Gdt.assert_property(obj, prop_name, default_value, msg)

func assert_readonly_property(obj: Object, prop_name: String, msg: String = "") -> bool:
	return Gdt.assert_readonly_property(obj, prop_name, msg)

func assert_property_with_range(obj: Object, prop_name: String, min_val: float, max_val: float, msg: String = "") -> bool:
	return Gdt.assert_property_with_range(obj, prop_name, min_val, max_val, msg)

func assert_push_error(text: String, msg: String = "") -> bool:
	return Gdt.assert_push_error(text, msg)

func assert_push_warning(text: String, msg: String = "") -> bool:
	return Gdt.assert_push_warning(text, msg)

func assert_no_push_error(msg: String = "") -> bool:
	return Gdt.assert_no_push_error(msg)

# ── Signal watchers ─────────────────────────────────────────────────

func watch_signal(obj: Object, signal_name: String) -> Object:
	return Gdt.watch_signal(obj, signal_name)

func assert_emitted_before(obj_a: Object, sig_a: String, obj_b: Object, sig_b: String, msg: String = "") -> bool:
	return Gdt.assert_emitted_before(obj_a, sig_a, obj_b, sig_b, msg)

func assert_signal_order(steps: Array, msg: String = "") -> bool:
	return Gdt.assert_signal_order(steps, msg)

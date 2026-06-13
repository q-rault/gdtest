class_name Gdt
extends RefCounted

# gdt — user-facing assertion + async helpers for gdtest tests.
#
# Drop harness/gdt.gd and harness/gdtest_worker.gd into your project
# (typically under `addons/gdtest/`). Then in any `test_*.gd` file,
# call `Gdt.expect(...)` / `Gdt.expect_eq(...)` — no imports needed,
# `class_name Gdt` registers the symbol globally.
#
# All helpers forward to the running harness, which the orchestrator
# exposes via `Engine.set_meta("gdt_harness", ...)`. Outside gdtest
# (e.g. in the editor) these calls are no-ops that return true/0 so
# importing the file in a utility script doesn't blow up.

class GdtMatcher:
	extends RefCounted
	var _fn: Callable
	func _init(fn: Callable) -> void:
		_fn = fn
	func matches(value: Variant) -> bool:
		return _fn.call(value)

class SignalWatcher extends RefCounted:
	var emissions: Array = []
	var emission_seqs: Array = []
	var _target: Object
	var _signal_name: String
	var _cb: Callable

	var count: int:
		get:
			return emissions.size()

	var last_args: Array:
		get:
			return emissions.back() if not emissions.is_empty() else []

	func _record_emission(args: Array) -> void:
		var h: Object = Engine.get_meta("gdt_harness") if Engine.has_meta("gdt_harness") else null
		emission_seqs.append(h.next_emission_seq() if h != null else emission_seqs.size())
		emissions.append(args)

	func reset() -> void:
		emissions.clear()
		emission_seqs.clear()

	func _disconnect() -> void:
		if not _cb.is_valid():
			return
		if is_instance_valid(_target) and _target.has_signal(_signal_name):
			if _target.is_connected(_signal_name, _cb):
				_target.disconnect(_signal_name, _cb)
		_cb = Callable()

class BenchResult extends RefCounted:
	var mean_ms: float = 0.0
	var median_ms: float = 0.0
	var p95_ms: float = 0.0
	var p99_ms: float = 0.0
	var min_ms: float = 0.0
	var max_ms: float = 0.0

	func _init(times_us: Array) -> void:
		if times_us.is_empty():
			return
		var sorted_us: Array = times_us.duplicate()
		sorted_us.sort()
		var n: int = sorted_us.size()
		min_ms = sorted_us[0] / 1000.0
		max_ms = sorted_us[n - 1] / 1000.0
		var total: float = 0.0
		for t in sorted_us:
			total += float(t)
		mean_ms = total / n / 1000.0
		median_ms = _percentile(sorted_us, 0.5)
		p95_ms = _percentile(sorted_us, 0.95)
		p99_ms = _percentile(sorted_us, 0.99)

	func _percentile(sorted_us: Array, p: float) -> float:
		var n: int = sorted_us.size()
		if n == 1:
			return sorted_us[0] / 1000.0
		var idx: float = p * float(n - 1)
		var lo: int = int(idx)
		var hi: int = mini(lo + 1, n - 1)
		var frac: float = idx - float(lo)
		return (float(sorted_us[lo]) * (1.0 - frac) + float(sorted_us[hi]) * frac) / 1000.0

class _IntGen extends RefCounted:
	var _min: int
	var _max: int
	func _init(mn: int, mx: int) -> void:
		_min = mn; _max = mx
	func generate(rng: RandomNumberGenerator) -> int:
		return rng.randi_range(_min, _max)
	func shrink(value: int) -> Array:
		if value == 0:
			return []
		var cs: Array = []
		if 0 >= _min and 0 <= _max:
			cs.append(0)
		var half: int = value / 2
		if half != value and half != 0:
			cs.append(half)
		var step: int = value - 1 if value > 0 else value + 1
		if step != 0 and step != half:
			cs.append(step)
		return cs

class _FloatGen extends RefCounted:
	var _min: float
	var _max: float
	func _init(mn: float, mx: float) -> void:
		_min = mn; _max = mx
	func generate(rng: RandomNumberGenerator) -> float:
		return rng.randf_range(_min, _max)
	func shrink(value: float) -> Array:
		if value == 0.0:
			return []
		var cs: Array = []
		if 0.0 >= _min and 0.0 <= _max:
			cs.append(0.0)
		var half: float = value / 2.0
		if half != 0.0:
			cs.append(half)
		return cs

class _StringGen extends RefCounted:
	var _max_len: int
	const _CHARSET := "abcdefghijklmnopqrstuvwxyz0123456789"
	func _init(max_len: int) -> void:
		_max_len = max_len
	func generate(rng: RandomNumberGenerator) -> String:
		var length: int = rng.randi_range(0, _max_len)
		var s: String = ""
		for _i in range(length):
			s += _CHARSET[rng.randi_range(0, _CHARSET.length() - 1)]
		return s
	func shrink(value: String) -> Array:
		if value.is_empty():
			return []
		var cs: Array = []
		cs.append("")
		if value.length() > 1:
			cs.append(value.substr(0, value.length() - 1))
		return cs

class _BoolGen extends RefCounted:
	func generate(rng: RandomNumberGenerator) -> bool:
		return rng.randi_range(0, 1) == 1
	func shrink(value: bool) -> Array:
		return [false] if value else []

class _ChoiceGen extends RefCounted:
	var _choices: Array
	func _init(choices: Array) -> void:
		_choices = choices
	func generate(rng: RandomNumberGenerator) -> Variant:
		return _choices[rng.randi_range(0, _choices.size() - 1)]
	func shrink(value: Variant) -> Array:
		if _choices.is_empty() or _choices[0] == value:
			return []
		return [_choices[0]]

# ── Plugin base classes (internal) ─────────────────────────────────────────
#
# Extend GdtReporter in an addon and list its path under `plugins` in
# gdtest.toml to receive test lifecycle events without modifying test files.
class GdtReporter:
	extends RefCounted
	## Called once before the first test starts.
	func on_run_start(total_tests: int) -> void: pass
	## Called just before a test function is invoked.
	func on_test_start(file: String, name: String) -> void: pass
	## Called when a test passes.
	func on_test_pass(file: String, name: String, us: int) -> void: pass
	## Called when a test has one or more assertion failures.
	func on_test_fail(file: String, name: String, us: int, failures: Array) -> void: pass
	## Called when a test is skipped (xtest_* or Gdt.skip()).
	func on_test_skip(file: String, name: String, message: String) -> void: pass
	## Called when a test errors (crash, timeout, etc.).
	func on_test_error(file: String, name: String, message: String) -> void: pass
	## Called once after the last test finishes.
	func on_run_end(passed: int, failed: int, skipped: int) -> void: pass

# Extend GdtFixture in an addon to hook into the test lifecycle at the
# suite level. Fixture plugins are called alongside the test instance's own
# before_each / after_each, so they can seed databases, reset state, etc.
class GdtFixture:
	extends RefCounted
	## Called once before the first test in a file runs.
	func before_suite() -> void: pass
	## Called once after the last test in a file runs.
	func after_suite() -> void: pass
	## Called before each test function.
	func before_each() -> void: pass
	## Called after each test function.
	func after_each() -> void: pass

# Fluent assertion chain returned by Gdt.assert_that(). Assertion libraries
# can extend this class and register custom methods via GdtAssertionChain.extend().
class GdtAssertionChain:
	extends RefCounted
	var _value: Variant
	var _h: Object  # harness reference (may be null outside gdtest)

	func _init(value: Variant, harness: Object) -> void:
		_value = value
		_h = harness

	func _fail(msg: String) -> void:
		if _h != null and _h.has_method("expect"):
			_h.expect(false, msg)

	func is_equal_to(expected: Variant, msg: String = "") -> Object:
		if _value != expected:
			_fail(msg if msg != "" else "expected %s == %s" % [var_to_str(_value), var_to_str(expected)])
		return self

	func is_not_equal_to(expected: Variant, msg: String = "") -> Object:
		if _value == expected:
			_fail(msg if msg != "" else "expected %s != %s" % [var_to_str(_value), var_to_str(expected)])
		return self

	func is_null(msg: String = "") -> Object:
		if _value != null:
			_fail(msg if msg != "" else "expected null, got %s" % var_to_str(_value))
		return self

	func is_not_null(msg: String = "") -> Object:
		if _value == null:
			_fail(msg if msg != "" else "expected non-null value")
		return self

	func is_true(msg: String = "") -> Object:
		if not _value:
			_fail(msg if msg != "" else "expected true, got %s" % var_to_str(_value))
		return self

	func is_false(msg: String = "") -> Object:
		if _value:
			_fail(msg if msg != "" else "expected false, got %s" % var_to_str(_value))
		return self

	func is_greater_than(bound: Variant, msg: String = "") -> Object:
		if not (_value > bound):
			_fail(msg if msg != "" else "expected %s > %s" % [var_to_str(_value), var_to_str(bound)])
		return self

	func is_less_than(bound: Variant, msg: String = "") -> Object:
		if not (_value < bound):
			_fail(msg if msg != "" else "expected %s < %s" % [var_to_str(_value), var_to_str(bound)])
		return self

	func is_between(low: Variant, high: Variant, msg: String = "") -> Object:
		if not (_value >= low and _value <= high):
			_fail(msg if msg != "" else "expected %s in [%s, %s]" % [var_to_str(_value), var_to_str(low), var_to_str(high)])
		return self

	func has_size(expected: int, msg: String = "") -> Object:
		var sz: int = _value.size() if _value != null else -1
		if sz != expected:
			_fail(msg if msg != "" else "expected size %d, got %d" % [expected, sz])
		return self

	func is_empty(msg: String = "") -> Object:
		var sz: int = _value.size() if _value != null else -1
		if sz != 0:
			_fail(msg if msg != "" else "expected empty collection, size was %d" % sz)
		return self

	func is_not_empty(msg: String = "") -> Object:
		var sz: int = _value.size() if _value != null else -1
		if sz == 0:
			_fail(msg if msg != "" else "expected non-empty collection")
		return self

	func contains(item: Variant, msg: String = "") -> Object:
		if not (_value != null and item in _value):
			_fail(msg if msg != "" else "expected %s to contain %s" % [var_to_str(_value), var_to_str(item)])
		return self

	static func _type_check(v: Variant, klass: Variant) -> bool:
		return is_instance_of(v, klass)

	func is_instance_of(klass: Variant, msg: String = "") -> Object:
		if not GdtAssertionChain._type_check(_value, klass):
			_fail(msg if msg != "" else "expected instance of %s, got %s" % [str(klass), _value.get_class() if _value != null else "null"])
		return self


static func _harness() -> Object:
	return Engine.get_meta("gdt_harness") if Engine.has_meta("gdt_harness") else null

# ── Typed asserter chains ────────────────────────────────────────────────────

class GdtBoolAssert:
	extends GdtAssertionChain

class GdtIntAssert:
	extends GdtAssertionChain

	func is_even(msg: String = "") -> Object:
		if int(_value) % 2 != 0:
			_fail(msg if msg != "" else "expected even number, got %s" % var_to_str(_value))
		return self

	func is_odd(msg: String = "") -> Object:
		if int(_value) % 2 != 1 and int(_value) % 2 != -1:
			_fail(msg if msg != "" else "expected odd number, got %s" % var_to_str(_value))
		return self

class GdtFloatAssert:
	extends GdtAssertionChain

	func is_equal_approx(expected: float, tolerance: float = 1e-6, msg: String = "") -> Object:
		var diff: float = absf(float(_value) - expected)
		if diff > tolerance:
			_fail(msg if msg != "" else "expected %s ≈ %s (tolerance %s, diff %s)" % [_value, expected, tolerance, diff])
		return self

	func is_nan(msg: String = "") -> Object:
		var f := float(_value)
		if not (f != f):
			_fail(msg if msg != "" else "expected NaN, got %s" % var_to_str(_value))
		return self

	func is_inf(msg: String = "") -> Object:
		var f := float(_value)
		if not (f == INF or f == -INF):
			_fail(msg if msg != "" else "expected Inf, got %s" % var_to_str(_value))
		return self

class GdtStringAssert:
	extends GdtAssertionChain

	func contains(expected: Variant, msg: String = "") -> Object:
		var needle := String(expected)
		if not (String(_value)).contains(needle):
			_fail(msg if msg != "" else 'expected "%s" to contain "%s"' % [_value, needle])
		return self

	func not_contains(expected: Variant, msg: String = "") -> Object:
		var needle := String(expected)
		if (String(_value)).contains(needle):
			_fail(msg if msg != "" else 'expected "%s" not to contain "%s"' % [_value, needle])
		return self

	func contains_ignoring_case(expected: String, msg: String = "") -> Object:
		if not (String(_value)).to_lower().contains(expected.to_lower()):
			_fail(msg if msg != "" else 'expected "%s" to contain "%s" (case-insensitive)' % [_value, expected])
		return self

	func not_contains_ignoring_case(expected: String, msg: String = "") -> Object:
		if (String(_value)).to_lower().contains(expected.to_lower()):
			_fail(msg if msg != "" else 'expected "%s" not to contain "%s" (case-insensitive)' % [_value, expected])
		return self

	func starts_with(expected: String, msg: String = "") -> Object:
		if not (String(_value)).begins_with(expected):
			_fail(msg if msg != "" else 'expected "%s" to start with "%s"' % [_value, expected])
		return self

	func ends_with(expected: String, msg: String = "") -> Object:
		if not (String(_value)).ends_with(expected):
			_fail(msg if msg != "" else 'expected "%s" to end with "%s"' % [_value, expected])
		return self

	func has_length(expected: int, msg: String = "") -> Object:
		var length: int = (String(_value)).length()
		if length != expected:
			_fail(msg if msg != "" else 'expected length %d, got %d for "%s"' % [expected, length, _value])
		return self

	func is_equal_ignoring_case(expected: String, msg: String = "") -> Object:
		if (String(_value)).to_lower() != expected.to_lower():
			_fail(msg if msg != "" else 'expected "%s" == "%s" (case-insensitive)' % [_value, expected])
		return self

	func is_not_equal_ignoring_case(expected: String, msg: String = "") -> Object:
		if (String(_value)).to_lower() == expected.to_lower():
			_fail(msg if msg != "" else 'expected "%s" != "%s" (case-insensitive)' % [_value, expected])
		return self

class GdtArrayAssert:
	extends GdtAssertionChain

	func contains(expected: Variant, msg: String = "") -> Object:
		var arr: Array = _value if _value is Array else []
		var items: Array = expected if expected is Array else [expected]
		for item in items:
			if not (item in arr):
				_fail(msg if msg != "" else "expected array to contain %s" % var_to_str(item))
		return self

	func not_contains(expected: Variant, msg: String = "") -> Object:
		var arr: Array = _value if _value is Array else []
		var items: Array = expected if expected is Array else [expected]
		for item in items:
			if item in arr:
				_fail(msg if msg != "" else "expected array not to contain %s" % var_to_str(item))
		return self

	func contains_exactly(expected: Array, msg: String = "") -> Object:
		var arr: Array = _value if _value is Array else []
		if arr != expected:
			_fail(msg if msg != "" else "expected %s == %s (order matters)" % [var_to_str(arr), var_to_str(expected)])
		return self

	func contains_exactly_in_any_order(expected: Array, msg: String = "") -> Object:
		var arr: Array = (_value if _value is Array else []).duplicate()
		var exp: Array = expected.duplicate()
		arr.sort()
		exp.sort()
		if arr != exp:
			_fail(msg if msg != "" else "expected %s to contain exactly %s (any order)" % [var_to_str(_value), var_to_str(expected)])
		return self

class GdtDictAssert:
	extends GdtAssertionChain

	func contains_keys(expected: Array, msg: String = "") -> Object:
		var d: Dictionary = _value if _value is Dictionary else {}
		for key in expected:
			if not d.has(key):
				_fail(msg if msg != "" else "expected dict to contain key %s" % var_to_str(key))
		return self

	func not_contains_keys(expected: Array, msg: String = "") -> Object:
		var d: Dictionary = _value if _value is Dictionary else {}
		for key in expected:
			if d.has(key):
				_fail(msg if msg != "" else "expected dict not to contain key %s" % var_to_str(key))
		return self

	func contains_key_value(key: Variant, value: Variant, msg: String = "") -> Object:
		var d: Dictionary = _value if _value is Dictionary else {}
		if not d.has(key) or d[key] != value:
			_fail(msg if msg != "" else "expected dict[%s] == %s" % [var_to_str(key), var_to_str(value)])
		return self

class GdtObjectAssert:
	extends GdtAssertionChain

	func is_instanceof(klass: Variant, msg: String = "") -> Object:
		if not is_instance_of(_value, klass):
			_fail(msg if msg != "" else "expected instance of %s, got %s" % [str(klass), _value.get_class() if _value != null else "null"])
		return self

	func is_not_instanceof(klass: Variant, msg: String = "") -> Object:
		if is_instance_of(_value, klass):
			_fail(msg if msg != "" else "expected value NOT to be instance of %s" % str(klass))
		return self

class GdtVector2Assert:
	extends GdtAssertionChain

	func is_equal_approx(expected: Vector2, tolerance: float = 1e-5, msg: String = "") -> Object:
		var dist: float = (_value as Vector2).distance_to(expected)
		if dist > tolerance:
			_fail(msg if msg != "" else "expected %s ≈ %s (distance %s, tolerance %s)" % [_value, expected, dist, tolerance])
		return self

class GdtVector3Assert:
	extends GdtAssertionChain

	func is_equal_approx(expected: Vector3, tolerance: float = 1e-5, msg: String = "") -> Object:
		var dist: float = (_value as Vector3).distance_to(expected)
		if dist > tolerance:
			_fail(msg if msg != "" else "expected %s ≈ %s (distance %s, tolerance %s)" % [_value, expected, dist, tolerance])
		return self

class GdtSignalAssert:
	extends RefCounted
	var _node: Object
	var _h: Object

	func _init(node: Object, harness: Object) -> void:
		_node = node
		_h = harness

	func is_emitted(sig: String, timeout_ms: float = 1000.0) -> Object:
		if _h != null:
			await _h.watch_signal(_node, sig, timeout_ms, Callable())
		return self

	func is_not_emitted(sig: String, timeout_ms: float = 200.0) -> Object:
		if _h != null:
			await _h.watch_no_signal(_node, sig, timeout_ms)
		return self

class GdtFileAssert:
	extends RefCounted
	var _path: String
	var _h: Object

	func _init(path: String, harness: Object) -> void:
		_path = path
		_h = harness

	func _fail(msg: String) -> void:
		if _h != null and _h.has_method("expect"):
			_h.expect(false, msg)

	func _abs_path() -> String:
		return ProjectSettings.globalize_path(_path) if _path.begins_with("res://") or _path.begins_with("user://") else _path

	func _file_exists() -> bool:
		return FileAccess.file_exists(_path) or FileAccess.file_exists(_abs_path())

	func _dir_exists() -> bool:
		return DirAccess.dir_exists_absolute(_abs_path())

	func exists(msg: String = "") -> Object:
		if not (_file_exists() or _dir_exists()):
			_fail(msg if msg != "" else "expected path to exist: %s" % _path)
		return self

	func is_file(msg: String = "") -> Object:
		if not _file_exists():
			_fail(msg if msg != "" else "expected file to exist: %s" % _path)
		return self

	func is_directory(msg: String = "") -> Object:
		if not _dir_exists():
			_fail(msg if msg != "" else "expected directory to exist: %s" % _path)
		return self

	func is_empty(msg: String = "") -> Object:
		if _file_exists():
			var file := FileAccess.open(_path, FileAccess.READ)
			if file == null:
				file = FileAccess.open(_abs_path(), FileAccess.READ)
			if file == null:
				_fail(msg if msg != "" else "could not read file: %s" % _path)
				return self
			var is_empty_file: bool = file.get_length() == 0
			file.close()
			if not is_empty_file:
				_fail(msg if msg != "" else "expected file to be empty: %s" % _path)
			return self
		if _dir_exists():
			var dir := DirAccess.open(_abs_path())
			if dir == null:
				_fail(msg if msg != "" else "could not open directory: %s" % _path)
				return self
			dir.list_dir_begin()
			var entry := dir.get_next()
			while entry != "":
				if entry != "." and entry != "..":
					dir.list_dir_end()
					_fail(msg if msg != "" else "expected directory to be empty: %s" % _path)
					return self
				entry = dir.get_next()
			dir.list_dir_end()
			return self
		_fail(msg if msg != "" else "expected path to exist: %s" % _path)
		return self

	func has_content(text: String, msg: String = "") -> Object:
		if not _file_exists():
			_fail(msg if msg != "" else "expected file to exist: %s" % _path)
			return self
		var file := FileAccess.open(_path, FileAccess.READ)
		if file == null:
			file = FileAccess.open(_abs_path(), FileAccess.READ)
		if file == null:
			_fail(msg if msg != "" else "could not read file: %s" % _path)
			return self
		var content: String = file.get_as_text()
		file.close()
		if not content.contains(text):
			_fail(msg if msg != "" else 'expected file "%s" to contain "%s"' % [_path, text])
		return self

# ── Core assertion ────────────────────────────────────────────────────────

# Appends a failure message when `cond` is false. Returns `cond` so tests
# can early-return on a fatal assertion.
static func expect(cond: bool, msg: String = "") -> bool:
	var h: Object = _harness()
	if h == null:
		return cond
	return h.expect(cond, msg)

# ── Equality / comparison ─────────────────────────────────────────────────

static func expect_eq(actual: Variant, expected: Variant, msg: String = "") -> bool:
	if actual == expected:
		return true
	if msg != "":
		return expect(false, _interpolate_msg(msg, actual, expected))
	var diff := _diff_message(actual, expected)
	return expect(false, diff if diff != "" else "expected %s == %s" % [str(expected), str(actual)])

static func expect_ne(actual: Variant, not_expected: Variant, msg: String = "") -> bool:
	if actual != not_expected:
		return true
	var m: String = _interpolate_msg(msg, actual, not_expected) if msg != "" else \
		"expected %s != %s" % [str(actual), str(not_expected)]
	return expect(false, m)

static func expect_lt(actual: Variant, bound: Variant, msg: String = "") -> bool:
	if actual < bound:
		return true
	var m: String = _interpolate_msg(msg, actual, bound) if msg != "" else \
		"expected %s < %s" % [str(actual), str(bound)]
	return expect(false, m)

static func expect_lte(actual: Variant, bound: Variant, msg: String = "") -> bool:
	if actual <= bound:
		return true
	var m: String = _interpolate_msg(msg, actual, bound) if msg != "" else \
		"expected %s <= %s" % [str(actual), str(bound)]
	return expect(false, m)

static func expect_gt(actual: Variant, bound: Variant, msg: String = "") -> bool:
	if actual > bound:
		return true
	var m: String = _interpolate_msg(msg, actual, bound) if msg != "" else \
		"expected %s > %s" % [str(actual), str(bound)]
	return expect(false, m)

static func expect_gte(actual: Variant, bound: Variant, msg: String = "") -> bool:
	if actual >= bound:
		return true
	var m: String = _interpolate_msg(msg, actual, bound) if msg != "" else \
		"expected %s >= %s" % [str(actual), str(bound)]
	return expect(false, m)

# Float compare within tolerance.
static func expect_approx(actual: float, expected: float, tolerance: float = 1e-6, msg: String = "") -> bool:
	var diff: float = absf(actual - expected)
	var m: String = msg if msg != "" else "expected %s ≈ %s (tolerance %s, diff %s)" % [actual, expected, tolerance, diff]
	return expect(diff <= tolerance, m)

# ── Boolean / null ────────────────────────────────────────────────────────

static func expect_true(cond: bool, msg: String = "") -> bool:
	return expect(cond, msg if msg != "" else "expected true")

static func expect_false(cond: bool, msg: String = "") -> bool:
	return expect(not cond, msg if msg != "" else "expected false")

static func expect_null(value: Variant, msg: String = "") -> bool:
	var m: String = msg if msg != "" else "expected null, got %s" % str(value)
	return expect(value == null, m)

static func expect_not_null(value: Variant, msg: String = "") -> bool:
	return expect(value != null, msg if msg != "" else "expected non-null")

# Runs `checks` and returns true if no new failures were recorded inside it.
# All assertions in the callable run even when earlier ones fail, so callers
# get a complete picture of all broken conditions at once. The return value
# lets tests bail out after a critical block:
#
#   if not Gdt.assert_all(func():
#       assert_eq(player.hp, 100)
#       assert_eq(player.mana, 50)
#   ): return   # setup is broken; skip the rest
static func assert_all(checks: Callable) -> bool:
	var h: Object = _harness()
	var before: int = h.failure_count() if h != null else 0
	checks.call()
	var after: int = h.failure_count() if h != null else 0
	return after == before

# Always records a failure. Useful as an unreachable marker.
static func fail(msg: String = "explicit fail") -> void:
	expect(false, msg)

# Marks the current test as pending. Does not stop execution — add `return`
# after this call if you don't want the rest of the test body to run.
static func pending(msg: String = "not yet implemented") -> void:
	var h: Object = _harness()
	if h != null:
		h.pending(msg)

# Marks the current test as skipped. Does not stop execution — add `return`
# after this call if you don't want the rest of the test body to run.
static func skip(msg: String = "") -> void:
	var h: Object = _harness()
	if h != null:
		h.skip(msg)

# ── Async helpers ─────────────────────────────────────────────────────────

# Awaits `n` physics frames.
static func wait_physics_frames(n: int = 1) -> void:
	var h: Object = _harness()
	if h == null:
		return
	await h.simulate_physics_frames(n)

# Awaits the equivalent of `seconds` at the project physics tick rate.
static func wait_physics_seconds(seconds: float) -> void:
	var h: Object = _harness()
	if h == null:
		return
	await h.simulate_physics_seconds(seconds)

# Awaits one process frame (rarely needed — prefer wait_physics_frames for
# deterministic tests).
static func wait_process_frame() -> void:
	var h: Object = _harness()
	if h == null:
		return
	await h.simulate_process_frame()

# Polls `predicate` every process frame until it returns true or `timeout_ms`
# elapses. Records a test failure on timeout.
static func wait_until(predicate: Callable, timeout_ms: float = 2000.0) -> void:
	var h: Object = _harness()
	if h == null:
		return
	await h.wait_until(predicate, timeout_ms)

# Polls `predicate` every process frame until it returns false or `timeout_ms`
# elapses. Records a test failure on timeout.
static func wait_while(predicate: Callable, timeout_ms: float = 2000.0) -> void:
	var h: Object = _harness()
	if h == null:
		return
	await h.wait_while(predicate, timeout_ms)

# Sets physics_ticks_per_second so that _physics_process receives `seconds` as
# delta. Auto-restored after each test. Call reset_physics_delta() to restore early.
static func set_physics_delta(seconds: float) -> void:
	var h: Object = _harness()
	if h == null:
		return
	h.set_physics_delta(seconds)

# Restores physics_ticks_per_second to the value before set_physics_delta().
static func reset_physics_delta() -> void:
	var h: Object = _harness()
	if h == null:
		return
	h.reset_physics_delta()

# ── Scene tree helpers (internal) ──────────────────────────────────────────

# Adds `node` under the per-test container so it gets freed automatically
# when the test ends. Returns the node so you can chain:
#     var hc := Gdt.add_child_autofree(HealthComponent.new())
# Outside a running test (e.g. used in editor utilities), no-op that
# returns the node unchanged.
static func add_child_autofree(node: Node) -> Node:
	var h: Object = _harness()
	if h == null:
		return node
	return h.add_child_autofree(node)

# Instantiates `packed` and parents the result under the per-test
# container, returning the instance ready to use. Equivalent to
# `Gdt.add_child_autofree(packed.instantiate())`.
static func instantiate_scene(packed: PackedScene) -> Node:
	var h: Object = _harness()
	if h == null:
		return packed.instantiate()
	return h.instantiate_scene(packed)

# ── Fluent assertion chain ────────────────────────────────────────────────

static func assert_that(value: Variant) -> Object:
	return GdtAssertionChain.new(value, _harness())

static func assert_bool(value: bool) -> GdtBoolAssert:
	return GdtBoolAssert.new(value, _harness())

static func assert_int(value: int) -> GdtIntAssert:
	return GdtIntAssert.new(value, _harness())

static func assert_float(value: float) -> GdtFloatAssert:
	return GdtFloatAssert.new(value, _harness())

static func assert_str(value: String) -> GdtStringAssert:
	return GdtStringAssert.new(value, _harness())

static func assert_array(value: Array) -> GdtArrayAssert:
	return GdtArrayAssert.new(value, _harness())

static func assert_dict(value: Dictionary) -> GdtDictAssert:
	return GdtDictAssert.new(value, _harness())

static func assert_object(value: Object) -> GdtObjectAssert:
	return GdtObjectAssert.new(value, _harness())

static func assert_vector2(value: Vector2) -> GdtVector2Assert:
	return GdtVector2Assert.new(value, _harness())

static func assert_vector3(value: Vector3) -> GdtVector3Assert:
	return GdtVector3Assert.new(value, _harness())

static func assert_signal_of(node: Object) -> GdtSignalAssert:
	return GdtSignalAssert.new(node, _harness())

# ── Collection / type / range / string ───────────────────────────────────

static func assert_has(collection: Variant, value: Variant, msg: String = "") -> bool:
	var m: String = msg if msg != "" else "expected %s to contain %s" % [str(collection), str(value)]
	return expect(value in collection, m)

static func assert_not_has(collection: Variant, value: Variant, msg: String = "") -> bool:
	var m: String = msg if msg != "" else "expected %s not to contain %s" % [str(collection), str(value)]
	return expect(not (value in collection), m)

static func assert_is(obj: Variant, klass: Variant, msg: String = "") -> bool:
	var m: String = msg if msg != "" else "expected value to be instance of %s" % str(klass)
	return expect(is_instance_of(obj, klass), m)

static func assert_between(value: Variant, low: Variant, high: Variant, msg: String = "") -> bool:
	var m: String = msg if msg != "" else "expected %s to be between %s and %s" % [str(value), str(low), str(high)]
	return expect(value >= low and value <= high, m)

static func assert_string_contains(haystack: String, needle: String, msg: String = "") -> bool:
	var m: String = msg if msg != "" else 'expected "%s" to contain "%s"' % [haystack, needle]
	return expect(haystack.contains(needle), m)

static func assert_string_starts_with(s: String, prefix: String, msg: String = "") -> bool:
	var m: String = msg if msg != "" else 'expected "%s" to start with "%s"' % [s, prefix]
	return expect(s.begins_with(prefix), m)

static func assert_string_ends_with(s: String, suffix: String, msg: String = "") -> bool:
	var m: String = msg if msg != "" else 'expected "%s" to end with "%s"' % [s, suffix]
	return expect(s.ends_with(suffix), m)

static func assert_file(path: String) -> GdtFileAssert:
	return GdtFileAssert.new(path, _harness())

static func _temp_dir_root() -> String:
	return OS.get_temp_dir().path_join("gdtest")

static func _temp_dir_path(name: String) -> String:
	return _temp_dir_root().path_join(name)

static func create_temp_dir(name: String) -> String:
	var path := _temp_dir_path(name)
	delete_temp_dir(name)
	DirAccess.make_dir_recursive_absolute(path)
	return path

static func delete_temp_dir(name: String) -> void:
	_delete_dir_recursive_absolute(ProjectSettings.globalize_path(_temp_dir_path(name)))

static func _delete_dir_recursive_absolute(abs_path: String) -> void:
	if not DirAccess.dir_exists_absolute(abs_path):
		return
	var dir := DirAccess.open(abs_path)
	if dir == null:
		return
	dir.list_dir_begin()
	var entry := dir.get_next()
	while entry != "":
		if entry != "." and entry != "..":
			var child_path := abs_path.path_join(entry)
			if dir.current_is_dir():
				_delete_dir_recursive_absolute(child_path)
			else:
				dir.remove(entry)
		entry = dir.get_next()
	dir.list_dir_end()
	var parent := DirAccess.open(abs_path.get_base_dir())
	if parent != null:
		parent.remove(abs_path.get_file())

# ── Godot spatial types ───────────────────────────────────────────────────

static func assert_vec2_approx(a: Vector2, b: Vector2, tolerance: float = 1e-5, msg: String = "") -> bool:
	var dist: float = a.distance_to(b)
	var m: String = msg if msg != "" else "expected %s ≈ %s (distance %s, tolerance %s)" % [a, b, dist, tolerance]
	return expect(dist <= tolerance, m)

static func assert_vec3_approx(a: Vector3, b: Vector3, tolerance: float = 1e-5, msg: String = "") -> bool:
	var dist: float = a.distance_to(b)
	var m: String = msg if msg != "" else "expected %s ≈ %s (distance %s, tolerance %s)" % [a, b, dist, tolerance]
	return expect(dist <= tolerance, m)

static func assert_vec4_approx(a: Vector4, b: Vector4, tolerance: float = 1e-5, msg: String = "") -> bool:
	var dist: float = (a - b).length()
	var m: String = msg if msg != "" else "expected %s ≈ %s (distance %s, tolerance %s)" % [a, b, dist, tolerance]
	return expect(dist <= tolerance, m)

static func assert_color_approx(a: Color, b: Color, tolerance: float = 1e-5, msg: String = "") -> bool:
	var max_diff: float = maxf(maxf(absf(a.r - b.r), absf(a.g - b.g)), maxf(absf(a.b - b.b), absf(a.a - b.a)))
	var m: String = msg if msg != "" else "expected color %s ≈ %s (max channel diff %s, tolerance %s)" % [a, b, max_diff, tolerance]
	return expect(max_diff <= tolerance, m)

static func assert_rect2_approx(a: Rect2, b: Rect2, tolerance: float = 1e-5, msg: String = "") -> bool:
	var pos_dist: float = a.position.distance_to(b.position)
	var size_dist: float = a.size.distance_to(b.size)
	var m: String = msg if msg != "" else "expected rect %s ≈ %s (pos dist %s, size dist %s, tolerance %s)" % [a, b, pos_dist, size_dist, tolerance]
	return expect(pos_dist <= tolerance and size_dist <= tolerance, m)

# ── Signal assertions ─────────────────────────────────────────────────────

# Awaits `sig` on `node` for up to `timeout_ms` ms; records a failure if it
# doesn't fire. Must be called with `await`:
#     await Gdt.assert_signal(my_node, "my_signal")
static func assert_signal_count(node: Object, sig: String, expected_count: int, timeout_ms: float = 1000.0) -> void:
	var h: Object = _harness()
	if h == null:
		return
	await h.watch_signal_count(node, sig, expected_count, timeout_ms)

static func assert_signal_with_args(node: Object, sig: String, expected_args: Array, timeout_ms: float = 1000.0) -> void:
	var h: Object = _harness()
	if h == null:
		return
	await h.watch_signal_with_args(node, sig, expected_args, timeout_ms)

static func assert_signal(node: Object, sig: String, timeout_ms: float = 1000.0, context_fn: Callable = Callable()) -> void:
	var h: Object = _harness()
	if h == null:
		return
	await h.watch_signal(node, sig, timeout_ms, context_fn)

# Waits `timeout_ms` ms and fails if `sig` fires during that window.
static func assert_no_signal(node: Object, sig: String, timeout_ms: float = 200.0) -> void:
	var h: Object = _harness()
	if h == null:
		return
	await h.watch_no_signal(node, sig, timeout_ms)

# ── Snapshot assertions (internal) ────────────────────────────────────────────

# Serializes `actual` with var_to_str and compares against a stored snapshot
# file. On the first call the file is created and the test passes. On
# subsequent calls the stored value must match exactly; if not, the test fails
# with a diff. Pass a unique `name` per snapshot within the same test; if
# omitted an auto-incrementing index is used. Caller should use the GdTest
# wrapper instead of calling this directly so `script_path` is filled in.
static func assert_snapshot(actual: Variant, name: String = "", script_path: String = "") -> bool:
	var h: Object = _harness()
	if h == null:
		return true
	return h.assert_snapshot(actual, name, script_path)

# ── Mocking / doubles ─────────────────────────────────────────────────────
#
# Creates a test double of any GDScript class. All user-defined methods are
# replaced by stubs that record calls and return configured values.
#
# Usage:
#   var hp = Gdt.mock(HealthComponent)
#   Gdt.stub(hp, "get_hp", 100)
#   subject.set_target(hp)
#   Gdt.assert_called(hp, "get_hp")
#   Gdt.assert_called_with(hp, "take_damage", [50])

# ── Argument matchers (internal) ───────────────────────────────────────────

static func any():
	return GdtMatcher.new(func(_v: Variant) -> bool: return true)

static func any_of_type(type_int: int):
	return GdtMatcher.new(func(v: Variant) -> bool: return typeof(v) == type_int)

static func any_int():
	return any_of_type(TYPE_INT)

static func any_float():
	return any_of_type(TYPE_FLOAT)

static func any_string():
	return any_of_type(TYPE_STRING)

static func any_bool():
	return any_of_type(TYPE_BOOL)

static func satisfying(fn: Callable):
	return GdtMatcher.new(fn)

static func matching(regex_str: String):
	var re := RegEx.create_from_string(regex_str)
	return GdtMatcher.new(func(v: Variant) -> bool: return v is String and re.search(v) != null)

# Returns true when every element of `actual` matches the corresponding
# element of `expected`, treating GdtMatcher entries as wildcard patterns.
static func _args_match(actual: Array, expected: Array) -> bool:
	if actual.size() != expected.size():
		return false
	for i in actual.size():
		var e: Variant = expected[i]
		if e is GdtMatcher:
			if not e.matches(actual[i]):
				return false
		elif actual[i] != e:
			return false
	return true

# Shared code generator for spy and partial_mock: subclass that records all
# calls and delegates to super when no stub is configured.
static func _build_passthrough_script(klass: GDScript, caller: String) -> GDScript:
	var extends_path: String = klass.resource_path
	if extends_path == "":
		push_error("Gdt.%s: script has no resource path (anonymous script not supported)" % caller)
		return null
	var methods: Array = klass.get_script_method_list()

	var src := 'extends "%s"\n\n' % extends_path
	src += "var __calls: Dictionary = {}\n"
	src += "var __stubs: Dictionary = {}\n"
	src += "var __stub_patterns: Array = []\n"
	src += "var __call_log: Array = []\n\n"
	src += "func __record(m: String, a: Array) -> void:\n"
	src += "\tif not __calls.has(m): __calls[m] = []\n"
	src += "\t__calls[m].append(a)\n"
	src += "\t__call_log.append([m, a])\n\n"
	src += "func __spy_stub(m: String, a: Array) -> Array:\n"
	src += '\tvar ka: String = str(m) + "|" + str(a)\n'
	src += "\tif __stubs.has(ka): return [true, __stubs[ka]]\n"
	src += "\tif __stubs.has(m): return [true, __stubs[m]]\n"
	src += "\tfor p in __stub_patterns:\n"
	src += '\t\tif p[0] == m and Gdt._args_match(a, p[1]): return [true, p[2]]\n'
	src += "\treturn [false, null]\n\n"

	for md in methods:
		var mname: String = md["name"]
		if mname.begins_with("_"):
			continue
		var arg_list: PackedStringArray = PackedStringArray()
		for i in md.get("args", []).size():
			arg_list.append("a%d" % i)
		var arg_str: String = ", ".join(arg_list)
		var arr_str: String = "[%s]" % ", ".join(arg_list) if arg_list.size() > 0 else "[]"
		var ret_type: int = md.get("return", {}).get("type", 0)
		src += "func %s(%s):\n" % [mname, arg_str]
		src += '\t__record("%s", %s)\n' % [mname, arr_str]
		src += '\tvar __s := __spy_stub("%s", %s)\n' % [mname, arr_str]
		src += "\tif __s[0]: return __s[1]\n"
		if ret_type != 0:
			src += "\treturn super.%s(%s)\n\n" % [mname, arg_str]
		else:
			src += "\tsuper.%s(%s)\n\n" % [mname, arg_str]

	var gen := GDScript.new()
	gen.source_code = src
	var err := gen.reload()
	if err != OK:
		push_error("Gdt.%s: failed to compile for '%s' (error %d)\nsource:\n%s" % [caller, extends_path, err, src])
		return null
	return gen

# Creates a spy around `real`: all methods run their real implementation and
# calls are recorded. Use `stub` to override specific methods.
# Returns null and pushes an error if the script has no resource path.
static func spy(real: Object, auto_free: bool = true) -> Object:
	var klass: GDScript = real.get_script() as GDScript
	if klass == null:
		push_error("Gdt.spy: object has no GDScript")
		return null
	var gen := _build_passthrough_script(klass, "spy")
	if gen == null:
		return null
	var spy_obj: Object = gen.new()
	for prop in real.get_property_list():
		if prop["usage"] & PROPERTY_USAGE_SCRIPT_VARIABLE:
			spy_obj.set(prop["name"], real.get(prop["name"]))
	if auto_free:
		var h: Object = _harness()
		if h != null:
			h.register_mock(spy_obj)
	return spy_obj

# Creates a partial mock of `klass`: a fresh instance where all methods call
# through to the real implementation unless explicitly stubbed. Calls are
# recorded for all methods regardless. Use `stub` to override specific methods.
static func partial_mock(klass: GDScript, auto_free: bool = true) -> Object:
	var gen := _build_passthrough_script(klass, "partial_mock")
	if gen == null:
		return null
	var obj: Object = gen.new()
	if auto_free:
		var h: Object = _harness()
		if h != null:
			h.register_mock(obj)
	return obj

# Replaces a user-defined autoload (GDScript node at /root/<name>) with a full
# mock for the duration of the current test. The original node is renamed out of
# the way; the mock takes its name so that GDScript `MyAutoload.method()` calls
# hit the mock. Auto-restored after after_each by the harness.
#
# NOTE: Built-in engine singletons (Input, OS, Time, Engine) are C++ objects
# and CANNOT be replaced this way. Only user-defined Project Settings autoloads
# are supported.
static func mock_singleton(name: String) -> Object:
	var h: Object = _harness()
	if h == null:
		push_error("Gdt.mock_singleton: no running test harness")
		return null
	var real_node: Node = Engine.get_main_loop().root.get_node_or_null(name) as Node
	if real_node == null:
		push_error("Gdt.mock_singleton: no autoload '%s' at /root/%s — built-in singletons (Input, OS, Time) cannot be doubled" % [name, name])
		return null
	var klass: GDScript = real_node.get_script() as GDScript
	if klass == null:
		push_error("Gdt.mock_singleton: '%s' has no GDScript — only user-defined autoloads can be doubled" % name)
		return null
	var mock_node := _build_autoload_mock(klass, name)
	if mock_node == null:
		return null
	real_node.name = "__gdtest_real_%s" % name
	mock_node.name = name
	Engine.get_main_loop().root.add_child(mock_node)
	h.register_singleton_swap(name, real_node, mock_node)
	return mock_node

static func _build_autoload_mock(klass: GDScript, singleton_name: String) -> Node:
	var extends_path: String = klass.resource_path
	if extends_path == "":
		push_error("Gdt.mock_singleton: '%s' script has no resource path" % singleton_name)
		return null
	var methods: Array = klass.get_script_method_list()

	var src := 'extends "%s"\n\n' % extends_path
	src += "var __calls: Dictionary = {}\n"
	src += "var __stubs: Dictionary = {}\n"
	src += "var __stub_patterns: Array = []\n"
	src += "var __call_log: Array = []\n\n"
	src += "func _ready() -> void: pass\n"
	src += "func _enter_tree() -> void: pass\n"
	src += "func _exit_tree() -> void: pass\n"
	src += "func _process(_d: float) -> void: pass\n"
	src += "func _physics_process(_d: float) -> void: pass\n\n"
	src += "func __record(m: String, a: Array) -> Variant:\n"
	src += "\tif not __calls.has(m): __calls[m] = []\n"
	src += "\t__calls[m].append(a)\n"
	src += "\t__call_log.append([m, a])\n"
	src += '\tvar ka: String = str(m) + "|" + str(a)\n'
	src += "\tif __stubs.has(ka): return __stubs[ka]\n"
	src += "\tif __stubs.has(m): return __stubs[m]\n"
	src += "\tfor p in __stub_patterns:\n"
	src += '\t\tif p[0] == m and Gdt._args_match(a, p[1]): return p[2]\n'
	src += "\treturn null\n\n"

	for md in methods:
		var mname: String = md["name"]
		if mname.begins_with("_"):
			continue
		var arg_list: PackedStringArray = PackedStringArray()
		for i in md.get("args", []).size():
			arg_list.append("a%d" % i)
		var arg_str: String = ", ".join(arg_list)
		var arr_str: String = "[%s]" % ", ".join(arg_list) if arg_list.size() > 0 else "[]"
		src += "func %s(%s):\n" % [mname, arg_str]
		src += '\treturn __record("%s", %s)\n\n' % [mname, arr_str]

	var gen := GDScript.new()
	gen.source_code = src
	var err := gen.reload()
	if err != OK:
		push_error("Gdt.mock_singleton: failed to compile mock for '%s' (error %d)\nsource:\n%s" % [singleton_name, err, src])
		return null
	return gen.new() as Node

# Creates a mock of `klass` (a GDScript resource, e.g. MyClass or load("...")).
# Returns null and pushes an error if compilation fails.
static func mock(klass: GDScript, auto_free: bool = true) -> Object:
	var methods: Array = klass.get_script_method_list()
	var extends_path: String = klass.resource_path

	var src := ""
	if extends_path != "":
		src += 'extends "%s"\n\n' % extends_path
	else:
		src += "extends RefCounted\n\n"
	src += "var __calls: Dictionary = {}\n"
	src += "var __stubs: Dictionary = {}\n"
	src += "var __stub_patterns: Array = []\n"
	src += "var __call_log: Array = []\n\n"
	src += "func __record(m: String, a: Array) -> Variant:\n"
	src += "\tif not __calls.has(m): __calls[m] = []\n"
	src += "\t__calls[m].append(a)\n"
	src += "\t__call_log.append([m, a])\n"
	src += '\tvar ka: String = str(m) + "|" + str(a)\n'
	src += "\tif __stubs.has(ka): return __stubs[ka]\n"
	src += "\tif __stubs.has(m): return __stubs[m]\n"
	src += "\tfor p in __stub_patterns:\n"
	src += '\t\tif p[0] == m and Gdt._args_match(a, p[1]): return p[2]\n'
	src += "\treturn null\n\n"

	for md in methods:
		var name: String = md["name"]
		# Skip lifecycle, private, and our internal methods.
		if name.begins_with("_"):
			continue
		var arg_list: PackedStringArray = PackedStringArray()
		for i in md.get("args", []).size():
			arg_list.append("a%d" % i)
		var arg_str: String = ", ".join(arg_list)
		var arr_str: String = "[%s]" % ", ".join(arg_list) if arg_list.size() > 0 else "[]"
		src += "func %s(%s):\n" % [name, arg_str]
		src += '\treturn __record("%s", %s)\n\n' % [name, arr_str]

	var gen := GDScript.new()
	gen.source_code = src
	var err := gen.reload()
	if err != OK:
		push_error("Gdt.mock: failed to compile double for '%s' (error %d)\nsource:\n%s" % [extends_path, err, src])
		return null
	var obj: Object = gen.new()
	if auto_free:
		var h: Object = _harness()
		if h != null:
			h.register_mock(obj)
	return obj

# Set the default return value for `method_name` on a mock.
static func stub(mock_obj: Object, method_name: String, value: Variant = null) -> void:
	if not mock_obj.has_method("__record"):
		push_error("Gdt.stub: object is not a Gdt mock or spy")
		return
	mock_obj.__stubs[method_name] = value

# Set a return value for a specific call signature (args must match exactly).
static func stub_with_args(mock_obj: Object, method_name: String, args: Array, value: Variant = null) -> void:
	if not mock_obj.has_method("__record"):
		push_error("Gdt.stub_with_args: object is not a Gdt mock or spy")
		return
	var _has_matcher := false
	for _a in args:
		if _a is GdtMatcher:
			_has_matcher = true
			break
	if _has_matcher:
		mock_obj.__stub_patterns.append([method_name, args, value])
	else:
		mock_obj.__stubs[str(method_name) + "|" + str(args)] = value

# Returns all recorded call arg-arrays for `method_name`.
static func get_calls(mock_obj: Object, method_name: String) -> Array:
	return mock_obj.__calls.get(method_name, []).duplicate()

# Returns the flat call log as [[method_name, args], ...] in call order.
static func get_call_log(mock_obj: Object) -> Array:
	return mock_obj.__call_log.duplicate()

# ── Mock assertions ───────────────────────────────────────────────────────

static func assert_called(mock_obj: Object, method_name: String, msg: String = "") -> bool:
	var count: int = mock_obj.__calls.get(method_name, []).size()
	var m: String = msg if msg != "" else "expected '%s' to have been called" % method_name
	return expect(count > 0, m)

static func assert_not_called(mock_obj: Object, method_name: String, msg: String = "") -> bool:
	var count: int = mock_obj.__calls.get(method_name, []).size()
	var m: String = msg if msg != "" else "expected '%s' not to have been called" % method_name
	return expect(count == 0, m)

static func assert_call_count(mock_obj: Object, method_name: String, expected: int, msg: String = "") -> bool:
	var count: int = mock_obj.__calls.get(method_name, []).size()
	var m: String = msg if msg != "" else "expected '%s' called %d time(s), got %d" % [method_name, expected, count]
	return expect(count == expected, m)

static func assert_called_with(mock_obj: Object, method_name: String, args: Array, msg: String = "") -> bool:
	var calls: Array = mock_obj.__calls.get(method_name, [])
	var found: bool = false
	for call_args in calls:
		if _args_match(call_args, args):
			found = true
			break
	var m: String = msg if msg != "" else "expected '%s' to have been called with %s" % [method_name, str(args)]
	return expect(found, m)

# Pure-bool helper: true if method_names appears as a subsequence in the call log.
static func _check_called_in_order(mock_obj: Object, method_names: Array) -> bool:
	var log: Array = mock_obj.__call_log
	var i := 0
	for entry in log:
		if i < method_names.size() and entry[0] == method_names[i]:
			i += 1
	return i == method_names.size()

# Verifies that the methods in `method_names` were called in that relative
# order. Other calls may be interleaved — this checks for a subsequence, not
# strict consecutive ordering. Fails on the first out-of-order or missing call.
static func assert_called_in_order(mock_obj: Object, method_names: Array, msg: String = "") -> bool:
	if _check_called_in_order(mock_obj, method_names):
		return true
	var log: Array = mock_obj.__call_log
	var i := 0
	for entry in log:
		if i < method_names.size() and entry[0] == method_names[i]:
			i += 1
	var m: String = msg if msg != "" else \
		"expected calls in order %s — matched %d/%d (next expected: '%s')" % \
		[str(method_names), i, method_names.size(), method_names[i]]
	return expect(false, m)

# ── Message templating (internal) ─────────────────────────────────────────────

# Substitutes {actual} and {expected} in a custom message string.
# Falls back to the raw message when neither placeholder is present.
static func _interpolate_msg(msg: String, actual: Variant, expected: Variant) -> String:
	return msg.replace("{actual}", str(actual)).replace("{expected}", str(expected))

# ── Collection failure diffs (internal) ──────────────────────────────────────

static func _diff_message(actual: Variant, expected: Variant) -> String:
	if actual is Array and expected is Array:
		return _array_diff_msg(actual as Array, expected as Array)
	if actual is Dictionary and expected is Dictionary:
		return _dict_diff_msg(actual as Dictionary, expected as Dictionary)
	return ""

static func _array_diff_msg(actual: Array, expected: Array) -> String:
	var lines: PackedStringArray = PackedStringArray()
	lines.append("  expected : %s" % str(expected))
	lines.append("  actual   : %s" % str(actual))
	var check: int = mini(actual.size(), expected.size())
	for i in range(check):
		if actual[i] != expected[i]:
			lines.append("  [%d]: expected %s, got %s" % [i, str(expected[i]), str(actual[i])])
	for i in range(check, expected.size()):
		lines.append("  [%d]: missing (expected %s)" % [i, str(expected[i])])
	for i in range(check, actual.size()):
		lines.append("  [%d]: extra element (got %s)" % [i, str(actual[i])])
	return "\n".join(lines)

static func _dict_diff_msg(actual: Dictionary, expected: Dictionary) -> String:
	var lines: PackedStringArray = PackedStringArray()
	lines.append("  expected : %s" % str(expected))
	lines.append("  actual   : %s" % str(actual))
	for key in expected:
		if key not in actual:
			lines.append("  %s: missing key (expected %s)" % [str(key), str(expected[key])])
		elif actual[key] != expected[key]:
			lines.append("  %s: expected %s, got %s" % [str(key), str(expected[key]), str(actual[key])])
	for key in actual:
		if key not in expected:
			lines.append("  %s: unexpected key (got %s)" % [str(key), str(actual[key])])
	return "\n".join(lines)

# ── Property / export assertions (internal) ───────────────────────────────────

# Asserts that `obj` has an @export property named `prop_name` of Variant `type`.
static func assert_exports(obj: Object, prop_name: String, type: int, msg: String = "") -> bool:
	var h: Object = _harness()
	if h == null:
		return true
	return h.assert_exports(obj, prop_name, type, msg)

# Asserts that the default value of `prop_name` on a fresh instance matches `default_value`.
static func assert_property(obj: Object, prop_name: String, default_value: Variant, msg: String = "") -> bool:
	var h: Object = _harness()
	if h == null:
		return true
	return h.assert_property(obj, prop_name, default_value, msg)

# Asserts that setting `prop_name` has no effect (no setter or no-op setter).
static func assert_readonly_property(obj: Object, prop_name: String, msg: String = "") -> bool:
	var h: Object = _harness()
	if h == null:
		return true
	return h.assert_readonly_property(obj, prop_name, msg)

# Asserts that `prop_name` is `@export_range(min_val, max_val, ...)`.
static func assert_property_with_range(obj: Object, prop_name: String, min_val: float, max_val: float, msg: String = "") -> bool:
	var h: Object = _harness()
	if h == null:
		return true
	return h.assert_property_with_range(obj, prop_name, min_val, max_val, msg)

# Pure-bool helpers for negative tests (don't pollute _current_failures).

static func _check_exports(obj: Object, prop_name: String, type: int) -> bool:
	for prop in obj.get_property_list():
		if prop["name"] == prop_name:
			return bool(prop["usage"] & PROPERTY_USAGE_EDITOR) and prop["type"] == type
	return false

static func _check_property(obj: Object, prop_name: String, default_value: Variant) -> bool:
	var script: GDScript = obj.get_script() as GDScript
	var fresh: Object = null
	if script != null and script.can_instantiate():
		fresh = script.new()
	elif ClassDB.can_instantiate(obj.get_class()):
		fresh = ClassDB.instantiate(obj.get_class())
	if fresh == null:
		return false
	var actual: Variant = fresh.get(prop_name)
	if not (fresh is RefCounted):
		if fresh is Node:
			fresh.queue_free()
		else:
			fresh.free()
	return actual == default_value

static func _check_readonly_property(obj: Object, prop_name: String) -> bool:
	var original: Variant = obj.get(prop_name)
	var probe: Variant
	match typeof(original):
		TYPE_INT: probe = int(original) + 99999
		TYPE_FLOAT: probe = float(original) + 99999.0
		TYPE_BOOL: probe = not bool(original)
		TYPE_STRING: probe = "__gdt_probe__"
		_: probe = null
	obj.set(prop_name, probe)
	var after: Variant = obj.get(prop_name)
	obj.set(prop_name, original)
	return after == original

static func _check_property_with_range(obj: Object, prop_name: String, min_val: float, max_val: float) -> bool:
	for prop in obj.get_property_list():
		if prop["name"] == prop_name:
			if prop["hint"] != PROPERTY_HINT_RANGE:
				return false
			var parts: PackedStringArray = prop["hint_string"].split(",")
			if parts.size() < 2:
				return false
			return float(parts[0]) == min_val and float(parts[1]) == max_val
	return false

# ── Sandbox / scenario testing (internal) ─────────────────────────────────────
#
# Creates an isolated test world: a Node container added to the scene tree
# with auto-cleanup after each test. Actors (Nodes) are spawned into it.
# Physics and process frame stepping reuse the harness coroutines.
#
# Usage:
#   var world = test_world()
#   var actor = world.spawn(MyNode2D).at(Vector2(100, 0))
#   await world.physics_frames(10)
#   assert_true(actor.instance.has_reached_target)

static func test_world() -> Object:
	var h: Object = _harness()
	if h == null:
		push_error("Gdt.test_world: no running test harness")
		return null
	return h.create_test_world()

# ── Memory tracking + autoref scoping (internal) ──────────────────────────────

# Registers `obj` for automatic cleanup after the test. Nodes are freed (or
# removed from tree); RefCounted refs are simply released; bare Objects are
# freed. Returns `obj` unchanged so you can chain: `var r = autoref(Res.new())`.
static func autoref(obj: Object) -> Object:
	var h: Object = _harness()
	if h != null:
		h.autoref(obj)
	return obj

# Returns the net static-memory change (bytes) since the test started.
# Negative values are possible due to GC. Use for asserting leak-free tests.
static func current_heap_delta() -> int:
	var h: Object = _harness()
	if h == null:
		return 0
	return h.current_heap_delta()

# ── Push error / warning capture (internal) ───────────────────────────────────
#
# Transparent capture of push_error() / push_warning() requires a GDExtension
# hook not available in pure GDScript. Use Gdt.push_error() / Gdt.push_warning()
# in testable code instead of the built-in globals to enable these assertions.
# Outside a running test the wrappers fall back to printerr().

# Records msg for assert_push_error(). Outside tests, prints to stderr.
static func push_error(msg: String) -> void:
	var h: Object = _harness()
	if h != null:
		h.capture_push_error(msg)
	else:
		printerr(msg)

# Records msg for assert_push_warning(). Outside tests, prints via print_rich.
static func push_warning(msg: String) -> void:
	var h: Object = _harness()
	if h != null:
		h.capture_push_warning(msg)
	else:
		print_rich("[color=yellow]WARNING:[/color] " + msg)

# Asserts at least one push_error matching `text` was captured this test.
# `text` may be a regex when wrapped in /…/ (e.g. "/dropped to -\\d+/").
static func assert_push_error(text: String, msg: String = "") -> bool:
	var h: Object = _harness()
	if h == null:
		return true
	return h.assert_push_error(text, msg)

# Asserts at least one push_warning matching `text` was captured this test.
static func assert_push_warning(text: String, msg: String = "") -> bool:
	var h: Object = _harness()
	if h == null:
		return true
	return h.assert_push_warning(text, msg)

# Asserts no push_error() calls were captured during this test.
static func assert_no_push_error(msg: String = "") -> bool:
	var h: Object = _harness()
	if h == null:
		return true
	return h.assert_no_push_error(msg)

# Pure-bool helpers for use in negative tests (don't pollute _current_failures).
static func _check_push_error(text: String) -> bool:
	var h: Object = _harness()
	if h == null:
		return true
	for e in h._captured_errors:
		if h._push_msg_matches(e, text):
			return true
	return false

static func _check_no_push_error() -> bool:
	var h: Object = _harness()
	if h == null:
		return true
	return h._captured_errors.is_empty()

# ── Orphan node detection (internal) ──────────────────────────────────────────

# Fails if any orphan nodes were created since before_each. Baseline is taken
# automatically; call this anywhere inside a test body.
static func assert_no_new_orphans(msg: String = "") -> bool:
	var h: Object = _harness()
	if h == null:
		return true
	return h.assert_no_new_orphans(msg)

# ── Assert rewriting (internal) ───────────────────────────────────────────────
# Called by the assert rewriter in gdtest-analysis. Not for direct use.

static func _rewrite_eq(lhs: Variant, rhs: Variant, expr: String, file: String, line: int) -> void:
	if lhs == rhs:
		return
	var diff := _diff_message(lhs, rhs)
	if diff != "":
		expect(false, "assert(%s)  (%s:%d)\n%s" % [expr, file, line, diff])
	else:
		expect(false, "assert(%s)  (%s:%d)\n  left:  %s\n  right: %s" % [expr, file, line, str(lhs), str(rhs)])

static func _rewrite_ne(lhs: Variant, rhs: Variant, expr: String, file: String, line: int) -> void:
	if lhs != rhs:
		return
	expect(false, "assert(%s)  (%s:%d)\n  left:  %s\n  right: %s" % [expr, file, line, str(lhs), str(rhs)])

static func _rewrite_lt(lhs: Variant, rhs: Variant, expr: String, file: String, line: int) -> void:
	if lhs < rhs:
		return
	expect(false, "assert(%s)  (%s:%d)\n  left:  %s\n  right: %s" % [expr, file, line, str(lhs), str(rhs)])

static func _rewrite_lte(lhs: Variant, rhs: Variant, expr: String, file: String, line: int) -> void:
	if lhs <= rhs:
		return
	expect(false, "assert(%s)  (%s:%d)\n  left:  %s\n  right: %s" % [expr, file, line, str(lhs), str(rhs)])

static func _rewrite_gt(lhs: Variant, rhs: Variant, expr: String, file: String, line: int) -> void:
	if lhs > rhs:
		return
	expect(false, "assert(%s)  (%s:%d)\n  left:  %s\n  right: %s" % [expr, file, line, str(lhs), str(rhs)])

static func _rewrite_gte(lhs: Variant, rhs: Variant, expr: String, file: String, line: int) -> void:
	if lhs >= rhs:
		return
	expect(false, "assert(%s)  (%s:%d)\n  left:  %s\n  right: %s" % [expr, file, line, str(lhs), str(rhs)])

static func _rewrite_true(cond: bool, expr: String, file: String, line: int) -> void:
	if cond:
		return
	expect(false, "assert(%s)  (%s:%d)" % [expr, file, line])

# ── Signal watchers (internal) ─────────────────────────────────────────────────

# Returns a SignalWatcher that records every emission of `signal_name` on `obj`.
# Automatically disconnected after each test. Returns null on invalid signal.
static func watch_signal(obj: Object, signal_name: String) -> Object:
	if not obj.has_signal(signal_name):
		Gdt.push_error("Gdt.watch_signal: '%s' has no signal '%s'" % [str(obj), signal_name])
		return null
	var w := SignalWatcher.new()
	w._target = obj
	w._signal_name = signal_name
	var arg_count := 0
	for s in obj.get_signal_list():
		if s["name"] == signal_name:
			arg_count = s["args"].size()
			break
	var cb: Callable
	match arg_count:
		0: cb = func(): w._record_emission([])
		1: cb = func(a0): w._record_emission([a0])
		2: cb = func(a0, a1): w._record_emission([a0, a1])
		3: cb = func(a0, a1, a2): w._record_emission([a0, a1, a2])
		_: cb = func(a0, a1, a2, a3): w._record_emission([a0, a1, a2, a3])
	obj.connect(signal_name, cb)
	w._cb = cb
	var h: Object = _harness()
	if h != null:
		h.register_watcher_keyed(obj, signal_name, w)
	return w

# ── Signal order assertions (internal) ────────────────────────────────────────

# Pure-bool check: true if sig_a's first emission happened before sig_b's first.
static func _check_emitted_before(obj_a: Object, sig_a: String, obj_b: Object, sig_b: String) -> bool:
	var h: Object = _harness()
	if h == null:
		return true
	var wa: Object = h._find_watcher(obj_a, sig_a)
	var wb: Object = h._find_watcher(obj_b, sig_b)
	if wa == null or wb == null:
		return false
	if wa.emissions.is_empty() or wb.emissions.is_empty():
		return false
	return wa.emission_seqs[0] < wb.emission_seqs[0]

# Asserts that the first emission of sig_a occurred before the first of sig_b.
static func assert_emitted_before(obj_a: Object, sig_a: String, obj_b: Object, sig_b: String, msg: String = "") -> bool:
	var h: Object = _harness()
	if h == null:
		return true
	var wa: Object = h._find_watcher(obj_a, sig_a)
	var wb: Object = h._find_watcher(obj_b, sig_b)
	if wa == null:
		return expect(false, "assert_emitted_before: no watcher for '%s' — call watch_signal() first" % sig_a)
	if wb == null:
		return expect(false, "assert_emitted_before: no watcher for '%s' — call watch_signal() first" % sig_b)
	if wa.emissions.is_empty():
		return expect(false, "assert_emitted_before: '%s' was never emitted" % sig_a)
	if wb.emissions.is_empty():
		return expect(false, "assert_emitted_before: '%s' was never emitted" % sig_b)
	if wa.emission_seqs[0] < wb.emission_seqs[0]:
		return true
	var m: String = msg if msg != "" else \
		"'%s' did not fire before '%s' (emission #%d vs #%d)" % [sig_a, sig_b, wa.emission_seqs[0], wb.emission_seqs[0]]
	return expect(false, m)

# Pure-bool check: true if steps fired in the given order (subsequence of emission seqs).
static func _check_signal_order(steps: Array) -> bool:
	var h: Object = _harness()
	if h == null:
		return true
	var last_seq := -1
	for step in steps:
		var obj: Object = step.get("obj")
		var sig: String = step.get("sig", "")
		var w: Object = h._find_watcher(obj, sig)
		if w == null or w.emissions.is_empty():
			return false
		var seq: int = w.emission_seqs[0]
		if seq <= last_seq:
			return false
		last_seq = seq
	return true

# Asserts that signals in `steps` fired in the listed order.
# Each step is {obj: Object, sig: String}. Uses the first emission of each signal.
static func assert_signal_order(steps: Array, msg: String = "") -> bool:
	var h: Object = _harness()
	if h == null:
		return true
	var last_seq := -1
	var last_sig := ""
	for step in steps:
		var obj: Object = step.get("obj")
		var sig: String = step.get("sig", "")
		var w: Object = h._find_watcher(obj, sig)
		if w == null:
			return expect(false, "assert_signal_order: no watcher for '%s' — call watch_signal() first" % sig)
		if w.emissions.is_empty():
			return expect(false, "assert_signal_order: '%s' was never emitted" % sig)
		var seq: int = w.emission_seqs[0]
		if seq <= last_seq:
			var m: String = msg if msg != "" else \
				"signal order violated: '%s' (#%d) did not fire before '%s' (#%d)" % [last_sig, last_seq, sig, seq]
			return expect(false, m)
		last_seq = seq
		last_sig = sig
	return true

# ── Property-based testing (internal) ─────────────────────────────────────────

# Returns a generator that produces random integers in [min, max].
static func gen_int(mn: int, mx: int) -> Object:
	return _IntGen.new(mn, mx)

# Returns a generator that produces random floats in [min, max].
static func gen_float(mn: float, mx: float) -> Object:
	return _FloatGen.new(mn, mx)

# Returns a generator that produces random lowercase-alphanumeric strings up to max_len chars.
static func gen_string(max_len: int = 10) -> Object:
	return _StringGen.new(max_len)

# Returns a generator that produces random booleans.
static func gen_bool() -> Object:
	return _BoolGen.new()

# Returns a generator that picks a random element from choices.
static func gen_choice(choices: Array) -> Object:
	return _ChoiceGen.new(choices)

# Run callable with randomly generated inputs `runs` times, failing if any run fails.
# On failure, shrinks the input toward the minimal failing case and reports both.
# Set GDTEST_SEED env var (or --seed N CLI flag) to reproduce a specific failure.
static func forall(generators: Array, callable: Callable, runs: int = 100) -> bool:
	var h: Object = _harness()
	if h == null:
		return true
	var seed_str: String = OS.get_environment("GDTEST_SEED")
	var seed_val: int = int(seed_str) if seed_str != "" else Time.get_ticks_usec()
	var rng := RandomNumberGenerator.new()
	rng.seed = seed_val
	for run_idx in range(runs):
		var inputs: Array = []
		for gen in generators:
			inputs.append(gen.generate(rng))
		var before: int = h.failure_count()
		callable.callv(inputs)
		if h.failure_count() > before:
			h.trim_failures(before)
			var shrunk: Array = _shrink_inputs(generators, callable, inputs, h)
			var seed_info: String = " (seed=%d, run=%d)" % [seed_val, run_idx + 1]
			var orig_repr: String = _repr_inputs(inputs)
			if shrunk != inputs:
				return expect(false, "forall: failing case%s\n  original: %s\n  shrunk:   %s" % [seed_info, orig_repr, _repr_inputs(shrunk)])
			else:
				return expect(false, "forall: failing case%s: %s" % [seed_info, orig_repr])
	return true

static func _shrink_inputs(generators: Array, callable: Callable, failing: Array, h: Object) -> Array:
	var current: Array = failing.duplicate()
	var changed: bool = true
	while changed:
		changed = false
		for i in range(generators.size()):
			for candidate in generators[i].shrink(current[i]):
				var trial: Array = current.duplicate()
				trial[i] = candidate
				var before: int = h.failure_count()
				callable.callv(trial)
				var still_fails: bool = h.failure_count() > before
				h.trim_failures(before)
				if still_fails:
					current = trial
					changed = true
					break
	return current

static func _repr_inputs(inputs: Array) -> String:
	var parts: Array = []
	for v in inputs:
		parts.append(str(v))
	return "[" + ", ".join(parts) + "]"

# ── Benchmark API (internal) ──────────────────────────────────────────────────

# Runs `callable` N times and returns a BenchResult with timing statistics.
# Use inside bench_* test functions or any test_* that needs timing assertions.
static func bench(callable: Callable, iterations: int = 100) -> Object:
	var times: Array = []
	for _i in range(iterations):
		var t0: int = Time.get_ticks_usec()
		callable.call()
		times.append(Time.get_ticks_usec() - t0)
	var result: Object = BenchResult.new(times)
	return result

# Fails if the median run time exceeds max_ms.
# Accepts either a BenchResult (from bench()) or a Callable (runs bench internally).
static func assert_completes_in(result_or_callable, max_ms: float) -> bool:
	var r: Object
	if result_or_callable is Callable:
		r = bench(result_or_callable)
	else:
		r = result_or_callable
	if r.median_ms > max_ms:
		return expect(false, "assert_completes_in: median %.3f ms exceeds limit of %.3f ms" % [r.median_ms, max_ms])
	return true

# Fails if result_a.median_ms is not at least `factor` times faster than result_b.median_ms.
# factor=2.0 means result_a must be at least 2x faster.
static func assert_faster_than(result_a: Object, result_b: Object, factor: float = 1.0) -> bool:
	var threshold: float = result_b.median_ms / factor
	if result_a.median_ms > threshold:
		return expect(false, "assert_faster_than: %.3f ms is not %.1fx faster than %.3f ms (threshold %.3f ms)" % [
			result_a.median_ms, factor, result_b.median_ms, threshold
		])
	return true

# ── Coverage tracking (internal) ───────────────────────────────────────────────
# When --coverage is active, instrumented source files call _cov_hit() for each
# executed line. _cov_dump_file() is called by the worker before it exits.

static var _cov: Dictionary = {}

static func _cov_hit(file: String, line: int) -> void:
	if not _cov.has(file):
		_cov[file] = {}
	var fc: Dictionary = _cov[file]
	fc[line] = fc.get(line, 0) + 1

static func _cov_dump_file() -> void:
	if _cov.is_empty():
		return
	var project_root: String = ProjectSettings.globalize_path("res://")
	var pid: int = OS.get_process_id()
	var path: String = project_root + ".gdtest_cov_%d.json" % pid
	var file := FileAccess.open(path, FileAccess.WRITE)
	if file == null:
		return
	file.store_string(JSON.stringify(_cov))
	file.close()

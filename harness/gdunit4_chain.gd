class_name GdUnit4Chain
extends RefCounted

# GDUnit4 compat assertion chain.
# Self-contained: extends RefCounted directly to avoid cross-file inner-class
# inheritance issues in Godot's class cache. Reimplements GdtAssertionChain's
# base methods plus all GDUnit4 aliases and typed-specific methods.
# Only accessible via GdTestCompatGdUnit; not part of the canonical Gdt API.

var _value: Variant
var _h: Object

func _init(value: Variant, harness: Object) -> void:
	_value = value
	_h = harness

func _fail(msg: String) -> void:
	if _h != null and _h.has_method("expect"):
		_h.expect(false, msg)

# ── Base GdtAssertionChain methods ────────────────────────────────────────────

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

func is_equal_to(expected: Variant, msg: String = "") -> Object:
	if _value != expected:
		_fail(msg if msg != "" else "expected %s == %s" % [var_to_str(_value), var_to_str(expected)])
	return self

func is_not_equal_to(expected: Variant, msg: String = "") -> Object:
	if _value == expected:
		_fail(msg if msg != "" else "expected %s != %s" % [var_to_str(_value), var_to_str(expected)])
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

func _value_len() -> int:
	if _value == null:
		return -1
	if _value is String:
		return (String(_value)).length()
	if _value is Array or _value is Dictionary:
		return _value.size()
	return -1

func has_size(expected: int, msg: String = "") -> Object:
	var sz: int = _value_len()
	if sz != expected:
		_fail(msg if msg != "" else "expected size %d, got %d" % [expected, sz])
	return self

func is_empty(msg: String = "") -> Object:
	var sz: int = _value_len()
	if sz != 0:
		_fail(msg if msg != "" else "expected empty collection, size was %d" % sz)
	return self

func is_not_empty(msg: String = "") -> Object:
	var sz: int = _value_len()
	if sz == 0:
		_fail(msg if msg != "" else "expected non-empty collection")
	return self

static func _type_check(v: Variant, klass: Variant) -> bool:
	return is_instance_of(v, klass)

func is_instance_of(klass: Variant, msg: String = "") -> Object:
	if not _type_check(_value, klass):
		_fail(msg if msg != "" else "expected instance of %s, got %s" % [str(klass), _value.get_class() if _value != null else "null"])
	return self

# contains/not_contains: arrays check all elements, strings/others use `in`.
func contains(expected: Variant, msg: String = "") -> Object:
	if expected is Array and _value is Array:
		for item in (expected as Array):
			if not (item in (_value as Array)):
				_fail(msg if msg != "" else "expected array to contain %s" % var_to_str(item))
	elif not (_value != null and expected in _value):
		_fail(msg if msg != "" else "expected %s to contain %s" % [var_to_str(_value), var_to_str(expected)])
	return self

func not_contains(expected: Variant, msg: String = "") -> Object:
	if expected is Array and _value is Array:
		for item in (expected as Array):
			if item in (_value as Array):
				_fail(msg if msg != "" else "expected array not to contain %s" % var_to_str(item))
	elif _value != null and expected in _value:
		_fail(msg if msg != "" else "expected %s not to contain %s" % [var_to_str(_value), var_to_str(expected)])
	return self

func does_not_contain(item: Variant, msg: String = "") -> Object:
	return not_contains(item, msg)

# ── GDUnit4 chain aliases ─────────────────────────────────────────────────────

func is_equal(expected: Variant, msg: String = "") -> Object:
	return is_equal_to(expected, msg)

func is_not_equal(expected: Variant, msg: String = "") -> Object:
	return is_not_equal_to(expected, msg)

func is_greater(bound: Variant, msg: String = "") -> Object:
	return is_greater_than(bound, msg)

func is_greater_equal(bound: Variant, msg: String = "") -> Object:
	if not (_value >= bound):
		_fail(msg if msg != "" else "expected %s >= %s" % [var_to_str(_value), var_to_str(bound)])
	return self

func is_gte(bound: Variant, msg: String = "") -> Object:
	return is_greater_equal(bound, msg)

func is_less(bound: Variant, msg: String = "") -> Object:
	return is_less_than(bound, msg)

func is_less_equal(bound: Variant, msg: String = "") -> Object:
	if not (_value <= bound):
		_fail(msg if msg != "" else "expected %s <= %s" % [var_to_str(_value), var_to_str(bound)])
	return self

func is_lte(bound: Variant, msg: String = "") -> Object:
	return is_less_equal(bound, msg)

func is_instanceof(klass: Variant, msg: String = "") -> Object:
	if not _type_check(_value, klass):
		_fail(msg if msg != "" else "expected instance of %s, got %s" % [str(klass), _value.get_class() if _value != null else "null"])
	return self

func is_same(expected: Object, msg: String = "") -> Object:
	if _value != expected:
		_fail(msg if msg != "" else "expected same instance, got different objects")
	return self

func is_not_same(expected: Object, msg: String = "") -> Object:
	if _value == expected:
		_fail(msg if msg != "" else "expected different instances, got same object")
	return self

func is_zero(msg: String = "") -> Object:
	if _value != 0:
		_fail(msg if msg != "" else "expected 0, got %s" % var_to_str(_value))
	return self

func is_not_zero(msg: String = "") -> Object:
	if _value == 0:
		_fail(msg if msg != "" else "expected non-zero value")
	return self

func is_negative(msg: String = "") -> Object:
	if not (_value < 0):
		_fail(msg if msg != "" else "expected negative value, got %s" % var_to_str(_value))
	return self

func is_not_negative(msg: String = "") -> Object:
	if _value < 0:
		_fail(msg if msg != "" else "expected non-negative value, got %s" % var_to_str(_value))
	return self

func is_in(collection: Variant, msg: String = "") -> Object:
	if not (_value in collection):
		_fail(msg if msg != "" else "expected %s to be in %s" % [var_to_str(_value), var_to_str(collection)])
	return self

func is_not_in(collection: Variant, msg: String = "") -> Object:
	if _value in collection:
		_fail(msg if msg != "" else "expected %s not to be in %s" % [var_to_str(_value), var_to_str(collection)])
	return self

func is_not_instanceof(klass: Variant, msg: String = "") -> Object:
	if _type_check(_value, klass):
		_fail(msg if msg != "" else "expected value NOT to be instance of %s" % str(klass))
	return self

# ── Int ───────────────────────────────────────────────────────────────────────

func is_even(msg: String = "") -> Object:
	if int(_value) % 2 != 0:
		_fail(msg if msg != "" else "expected even number, got %s" % var_to_str(_value))
	return self

func is_odd(msg: String = "") -> Object:
	if int(_value) % 2 != 1 and int(_value) % 2 != -1:
		_fail(msg if msg != "" else "expected odd number, got %s" % var_to_str(_value))
	return self

# ── Float / Vector (type-dispatched) ─────────────────────────────────────────

func is_equal_approx(expected: Variant, tolerance: float = 1e-5, msg: String = "") -> Object:
	if _value is Vector2:
		var dist: float = (_value as Vector2).distance_to(expected)
		if dist > tolerance:
			_fail(msg if msg != "" else "expected %s ≈ %s" % [_value, expected])
	elif _value is Vector3:
		var dist: float = (_value as Vector3).distance_to(expected)
		if dist > tolerance:
			_fail(msg if msg != "" else "expected %s ≈ %s" % [_value, expected])
	else:
		var diff: float = absf(float(_value) - float(expected))
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

# ── String ────────────────────────────────────────────────────────────────────

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

# ── Array ─────────────────────────────────────────────────────────────────────

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

# ── Dict ──────────────────────────────────────────────────────────────────────

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

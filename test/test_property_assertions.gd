extends GdTest

const ExportedClass = preload("res://test/support/exported_class.gd")

# ── assert_exports ────────────────────────────────────────────────────────────

func test_assert_exports_float() -> void:
	var obj := ExportedClass.new()
	assert_exports(obj, "speed", TYPE_FLOAT)

func test_assert_exports_int() -> void:
	var obj := ExportedClass.new()
	assert_exports(obj, "damage", TYPE_INT)

func test_assert_exports_range_property() -> void:
	var obj := ExportedClass.new()
	assert_exports(obj, "health_pct", TYPE_FLOAT)

func test_assert_exports_wrong_type_fails() -> void:
	var obj := ExportedClass.new()
	assert_false(Gdt._check_exports(obj, "speed", TYPE_INT))

func test_assert_exports_non_exported_fails() -> void:
	var obj := ExportedClass.new()
	assert_false(Gdt._check_exports(obj, "not_exported", TYPE_INT))

func test_assert_exports_missing_prop_fails() -> void:
	var obj := ExportedClass.new()
	assert_false(Gdt._check_exports(obj, "nonexistent", TYPE_FLOAT))

# ── assert_property ───────────────────────────────────────────────────────────

func test_assert_property_default_float() -> void:
	var obj := ExportedClass.new()
	assert_property(obj, "speed", 50.0)

func test_assert_property_default_int() -> void:
	var obj := ExportedClass.new()
	assert_property(obj, "damage", 10)

func test_assert_property_wrong_default_fails() -> void:
	var obj := ExportedClass.new()
	assert_false(Gdt._check_property(obj, "speed", 99.0))

# ── assert_readonly_property ──────────────────────────────────────────────────

func test_assert_readonly_passes_for_no_op_setter() -> void:
	var obj := ExportedClass.new()
	assert_readonly_property(obj, "max_hp")

func test_assert_readonly_fails_for_writable_property() -> void:
	var obj := ExportedClass.new()
	assert_false(Gdt._check_readonly_property(obj, "speed"))

# ── assert_property_with_range ────────────────────────────────────────────────

func test_assert_property_with_range_passes() -> void:
	var obj := ExportedClass.new()
	assert_property_with_range(obj, "health_pct", 0.0, 100.0)

func test_assert_property_with_range_wrong_max_fails() -> void:
	var obj := ExportedClass.new()
	assert_false(Gdt._check_property_with_range(obj, "health_pct", 0.0, 50.0))

func test_assert_property_with_range_non_range_fails() -> void:
	var obj := ExportedClass.new()
	assert_false(Gdt._check_property_with_range(obj, "speed", 0.0, 100.0))

extends GdTest


func test_script_error_after_passing_assert() -> void:
	assert_eq(1, 1)
	var x: Object = null
	var _y: Variant = x.some_property

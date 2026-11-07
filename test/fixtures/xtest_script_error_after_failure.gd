extends GdTest


func test_failure_then_script_error() -> void:
	assert_eq(1, 2)
	var x: Object = null
	var _y: Variant = x.some_property

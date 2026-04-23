extends GdTest

func test_assert_has_array() -> void:
	assert_has([1, 2, 3], 2)

func test_assert_has_dict() -> void:
	assert_has({"a": 1, "b": 2}, "a")

func test_assert_not_has() -> void:
	assert_not_has([1, 2], 99)

func test_assert_not_has_dict() -> void:
	assert_not_has({"x": 1}, "z")

func test_assert_is_node() -> void:
	var n := Node.new()
	assert_is(n, Node)
	n.free()

func test_assert_is_refcounted() -> void:
	assert_is(RefCounted.new(), RefCounted)

func test_assert_between_int() -> void:
	assert_between(5, 1, 10)

func test_assert_between_float() -> void:
	assert_between(0.5, 0.0, 1.0)

func test_assert_string_contains() -> void:
	assert_string_contains("hello world", "world")

func test_assert_string_contains_at_start() -> void:
	assert_string_contains("gdtest rocks", "gdtest")

func test_assert_string_starts_with() -> void:
	assert_string_starts_with("gdtest", "gd")

func test_assert_string_ends_with() -> void:
	assert_string_ends_with("gdtest", "test")

func test_assert_ne() -> void:
	assert_ne(1, 2)
	assert_ne("foo", "bar")

func test_assert_null_and_not_null() -> void:
	assert_null(null)
	assert_not_null([])
	assert_not_null(0)

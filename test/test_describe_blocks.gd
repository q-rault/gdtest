extends GdTest

func test_top_level_passes() -> void:
	assert_true(true)

func test_top_level_eq() -> void:
	assert_eq(6 * 7, 42)


class describe_Arithmetic:
	extends GdTest

	func test_addition() -> void:
		assert_eq(1 + 1, 2)

	func test_subtraction() -> void:
		assert_eq(10 - 3, 7)

	func test_multiplication() -> void:
		assert_eq(3 * 4, 12)


class describe_Strings:
	extends GdTest

	func test_concat() -> void:
		assert_eq("hello" + " " + "world", "hello world")

	func test_length() -> void:
		assert_eq("gdtest".length(), 6)

	func test_uppercase() -> void:
		assert_eq("lower".to_upper(), "LOWER")


class describe_Collections:
	extends GdTest

	func test_array_size() -> void:
		var a := [1, 2, 3]
		assert_eq(a.size(), 3)

	func test_dict_has_key() -> void:
		var d := {"k": "v"}
		assert_has(d, "k")

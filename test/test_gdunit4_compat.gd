extends "res://harness/gdunit4_compat.gd"

# ── Lifecycle hooks ────────────────────────────────────────────────────────────

var _suite_counter: int = 0
var _test_counter: int = 0

func before() -> void:
	_suite_counter += 1

func after() -> void:
	_suite_counter -= 1

func before_test() -> void:
	_test_counter += 1

func after_test() -> void:
	_test_counter -= 1

func test_lifecycle_suite_hooks_ran() -> void:
	assert_eq(_suite_counter, 1, "before() ran exactly once")

func test_lifecycle_per_test_hooks_ran() -> void:
	assert_eq(_test_counter, 1, "before_test() ran for this test")

# ── assert_that chain — GDUnit4 canonical names ────────────────────────────────

func test_chain_is_equal() -> void:
	assert_that(42).is_equal(42)
	assert_that("hello").is_equal("hello")

func test_chain_is_not_equal() -> void:
	assert_that(1).is_not_equal(2)

func test_chain_is_greater_and_less() -> void:
	assert_that(5).is_greater(3)
	assert_that(3).is_less(5)
	assert_that(5).is_greater_equal(5)
	assert_that(3).is_less_equal(3)
	assert_that(5).is_gte(4)
	assert_that(3).is_lte(4)

func test_chain_is_null_not_null() -> void:
	assert_that(null).is_null()
	assert_that(42).is_not_null()

func test_chain_is_zero_negative_positive() -> void:
	assert_that(0).is_zero()
	assert_that(1).is_not_zero()
	assert_that(-1).is_negative()
	assert_that(0).is_not_negative()
	assert_that(1).is_not_negative()

func test_chain_is_in_not_in() -> void:
	assert_that(2).is_in([1, 2, 3])
	assert_that(5).is_not_in([1, 2, 3])

func test_chain_not_contains() -> void:
	assert_that([1, 2, 3]).not_contains(4)
	assert_that([1, 2, 3]).does_not_contain(99)

func test_chain_instanceof() -> void:
	var r := RefCounted.new()
	assert_that(r).is_instanceof(RefCounted)
	assert_that(r).is_instance_of(RefCounted)

# ── Typed asserters ────────────────────────────────────────────────────────────

func test_assert_bool() -> void:
	assert_bool(true).is_true()
	assert_bool(false).is_false()
	assert_bool(true).is_equal(true)

func test_assert_int() -> void:
	assert_int(42).is_equal(42)
	assert_int(42).is_greater(40).is_less(50)
	assert_int(42).is_greater_equal(42).is_less_equal(42)
	assert_int(0).is_zero()
	assert_int(1).is_not_zero()
	assert_int(-5).is_negative()
	assert_int(5).is_not_negative()
	assert_int(4).is_even()
	assert_int(3).is_odd()
	assert_int(42).is_between(40, 50)

func test_assert_float() -> void:
	assert_float(3.14).is_equal_approx(3.14, 0.01)
	assert_float(3.14).is_greater(3.0)
	assert_float(0.0).is_zero()
	assert_float(-1.0).is_negative()

func test_assert_str() -> void:
	assert_str("hello world").contains("world")
	assert_str("hello").not_contains("bye")
	assert_str("hello").starts_with("he")
	assert_str("hello").ends_with("lo")
	assert_str("hello").has_length(5)
	assert_str("hello").is_equal("hello")
	assert_str("Hello").is_equal_ignoring_case("hello")
	assert_str("").is_empty()
	assert_str("x").is_not_empty()

func test_assert_array() -> void:
	assert_array([1, 2, 3]).contains([1, 3])
	assert_array([1, 2, 3]).not_contains([4, 5])
	assert_array([1, 2, 3]).contains_exactly([1, 2, 3])
	assert_array([3, 1, 2]).contains_exactly_in_any_order([1, 2, 3])
	assert_array([1, 2, 3]).has_size(3)
	assert_array([]).is_empty()
	assert_array([1]).is_not_empty()

func test_assert_dict() -> void:
	var d := {"a": 1, "b": 2}
	assert_dict(d).contains_keys(["a", "b"])
	assert_dict(d).not_contains_keys(["c"])
	assert_dict(d).contains_key_value("a", 1)
	assert_dict(d).has_size(2)

func test_assert_object() -> void:
	var r := RefCounted.new()
	assert_object(r).is_not_null()
	assert_object(r).is_instanceof(RefCounted)
	assert_object(null).is_null()

func test_assert_vector2() -> void:
	assert_vector2(Vector2(1.0, 2.0)).is_equal_approx(Vector2(1.0001, 2.0001), 0.01)
	assert_vector2(Vector2.ZERO).is_equal(Vector2.ZERO)

func test_assert_vector3() -> void:
	assert_vector3(Vector3(1.0, 2.0, 3.0)).is_equal_approx(Vector3(1.0001, 2.0001, 3.0001), 0.01)

# ── GDUnit4 legacy aliases still available ────────────────────────────────────

func test_flat_assertions_available() -> void:
	assert_eq(1, 1)
	assert_ne(1, 2)
	assert_true(true)
	assert_false(false)
	assert_null(null)
	assert_not_null(1)
	assert_gt(2, 1)
	assert_gte(2, 2)
	assert_lt(1, 2)
	assert_lte(1, 1)
	assert_between(5, 1, 10)
	assert_has([1, 2], 1)
	assert_almost_eq(1.0, 1.0 + 1e-10, 1e-6)

func test_auto_free() -> void:
	var obj := RefCounted.new()
	var same := auto_free(obj)
	assert_eq(same, obj)

func test_assert_file_and_temp_dir_utils() -> void:
	var dir_path := create_temp_dir("gdunit4_compat_fs")
	var file_path := dir_path.path_join("note.txt")
	var file := FileAccess.open(file_path, FileAccess.WRITE)
	file.store_string("hello gdtest")
	file.close()

	assert_file(dir_path).exists().is_directory()
	assert_file(file_path).exists().is_file().has_content("hello")

	delete_temp_dir("gdunit4_compat_fs")

func test_assert_file_empty_directory_and_file() -> void:
	var dir_path := create_temp_dir("gdunit4_compat_empty")
	var file_path := dir_path.path_join("empty.txt")
	var file := FileAccess.open(file_path, FileAccess.WRITE)
	file.close()

	assert_file(file_path).is_empty()

	delete_temp_dir("gdunit4_compat_empty")
	dir_path = create_temp_dir("gdunit4_compat_empty")
	assert_file(dir_path).is_empty()
	delete_temp_dir("gdunit4_compat_empty")

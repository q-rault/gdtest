extends GdTest

func test_expect_eq() -> void:
	assert_eq(1 + 1, 2)
	assert_eq("foo" + "bar", "foobar")

func test_expect_ne_and_comparisons() -> void:
	assert_ne(3, 4)
	assert_lt(1, 2, "1 < 2")
	assert_gt(5, 3)
	assert_gte(5, 5)
	assert_lte(5, 5)

func test_expect_approx() -> void:
	assert_almost_eq(0.1 + 0.2, 0.3, 1e-9, "float fuzz")

func test_expect_bool_and_null() -> void:
	assert_true(2 * 2 == 4)
	assert_false(1 == 2)
	assert_null(null)
	assert_not_null([])

func test_async_with_gdt_wait() -> void:
	var before: int = Engine.get_physics_frames()
	await wait_physics_frames(2)
	var after: int = Engine.get_physics_frames()
	assert_eq(after - before, 2, "2 physics frames elapsed")

func test_file_assertions_and_temp_dir_utils() -> void:
	var dir_path := create_temp_dir("gdt_lib_fs")
	var file_path := dir_path.path_join("data.txt")
	var file := FileAccess.open(file_path, FileAccess.WRITE)
	file.store_string("alpha\nbeta")
	file.close()

	assert_file(dir_path).exists().is_directory()
	assert_file(file_path).exists().is_file().has_content("beta")

	delete_temp_dir("gdt_lib_fs")

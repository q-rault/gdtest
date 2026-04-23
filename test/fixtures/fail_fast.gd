extends GdTest

func test_first_fails_intentionally() -> void:
	assert_eq(1, 2, "intentional failure — used by cli_tests.sh to verify --fail-fast")

func test_second_would_pass() -> void:
	assert_true(true)

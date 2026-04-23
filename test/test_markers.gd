extends GdTest

# Verifies that pending() and skip() do not crash the harness.

func test_pending_does_not_crash() -> void:
	pending("not yet implemented")
	# pending() does not stop execution

func test_skip_does_not_crash() -> void:
	skip("intentionally skipping")
	return  # skip() does not stop execution; return to prevent assertions below

func test_normal_after_marker_tests_pass() -> void:
	assert_true(true)

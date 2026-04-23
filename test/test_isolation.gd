extends GdTest

# --isolation verification.
#
# Two strict checks: one passes only under --isolation=file, the other
# only under --isolation=test. Use --filter to run the matching pair:
#
#   gdtest test/test_isolation.gd -j 1 --filter 'survives'
#   gdtest test/test_isolation.gd -j 1 --filter 'gone' --isolation test
#
# `-j 1` is required so test_a and test_b land on the same worker
# under file isolation (otherwise the comparison is meaningless).

const _PID_KEY: String = "gdt_isolation_marker_pid"

func test_a_writes_pid_marker() -> void:
	assert_false(Engine.has_meta(_PID_KEY), "no leftover marker at start")
	Engine.set_meta(_PID_KEY, OS.get_process_id())

## @tag isolation-specific
func test_b_marker_survives_under_file_isolation() -> void:
	assert_true(Engine.has_meta(_PID_KEY),
		"marker missing — worker was respawned (use --isolation=file)")
	if Engine.has_meta(_PID_KEY):
		assert_eq(int(Engine.get_meta(_PID_KEY)), OS.get_process_id(),
			"PID should match — same worker process")

## @tag isolation-specific
func test_b_marker_gone_under_test_isolation() -> void:
	assert_false(Engine.has_meta(_PID_KEY),
		"marker survived — worker was NOT respawned (use --isolation=test)")

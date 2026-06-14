# File-header directive: every test defaults to per-test process isolation.
# Run with:
#   gdtest test/support/isolation_directives_fixture.gd -j 1
# Without `-j 1`, test_d may run in a different worker than test_c and fail,
# which demonstrates that `file` isolation means "reuse the worker if the same
# worker picks the next test", not "pin this file to one worker".
# gdtest: isolation=test

extends GdTest

const _KEY: String = "gdt_isolation_directives_pid"


func test_a_header_directive_starts_clean() -> void:
	assert_false(Engine.has_meta(_KEY))
	Engine.set_meta(_KEY, OS.get_process_id())


func test_b_header_directive_respawns_between_tests() -> void:
	assert_false(Engine.has_meta(_KEY))


## @isolation file
func test_c_function_directive_can_share_worker() -> void:
	assert_false(Engine.has_meta(_KEY))
	Engine.set_meta(_KEY, OS.get_process_id())


## @isolation file
func test_d_function_directive_reuses_worker() -> void:
	assert_true(Engine.has_meta(_KEY))
	if Engine.has_meta(_KEY):
		assert_eq(int(Engine.get_meta(_KEY)), OS.get_process_id())

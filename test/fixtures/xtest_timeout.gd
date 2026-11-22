extends RefCounted

# Run by test/cli_tests.sh with --timeout 1000 -j 1. Not a self-test: two of
# these tests go past the limit on purpose.

signal never_emitted


# Cancelled: the harness's waits return once gdtest cancels the test, so it
# ends and its worker takes the next test.
func test_a_waits_past_the_limit() -> void:
	var h: Object = Engine.get_meta("gdt_harness")
	await h.simulate_physics_seconds(30.0)
	Engine.set_meta("xtest_timeout_ran_to_end", true)


func test_b_runs_on_the_same_worker() -> void:
	var h: Object = Engine.get_meta("gdt_harness")
	h.expect(Engine.get_meta("xtest_timeout_ran_to_end", false), "the worker was kept")


# Not cancellable: no wait of the harness stops it, so its worker restarts.
func test_c_awaits_a_signal_never_emitted() -> void:
	await never_emitted


func test_d_runs_on_a_new_worker() -> void:
	var h: Object = Engine.get_meta("gdt_harness")
	h.expect(not Engine.get_meta("xtest_timeout_ran_to_end", false), "a new worker")

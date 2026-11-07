extends RefCounted

# Exercises crash correlation. A GDScript null-deref logs `SCRIPT ERROR`
# and aborts the test function mid-way. Without correlation, every test
# below would show up as `pass` because the subsequent `expect()` never
# executes — a silent false positive.

func test_silent_script_error_escalates() -> void:
	var h: Object = Engine.get_meta("gdt_harness")
	var x: Object = null
	printerr(char(27) + "[32m[gdtest] intentional SCRIPT ERROR below — null-deref to test crash correlation" + char(27) + "[0m")
	var _y: Variant = x.some_property
	h.expect(true, "never reached")

## @tag harness-meta
func test_fail_first_then_script_error() -> void:
	# expect() runs first (records a failure), then the null deref logs a
	# SCRIPT ERROR. Expected outcome: TestFail with the runtime error
	# appended to the failures list.
	var h: Object = Engine.get_meta("gdt_harness")
	h.expect(false, "explicit failure before the runtime error")
	var x: Object = null
	var _y: Variant = x.cannot_reach_this

func test_clean_passes() -> void:
	# Baseline: no runtime error, normal pass.
	var h: Object = Engine.get_meta("gdt_harness")
	h.expect(true, "clean")

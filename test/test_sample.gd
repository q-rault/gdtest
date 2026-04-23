extends RefCounted

# M1 sample test — proves gdtest can load, instantiate, dispatch, and report results.

func test_arithmetic_passes() -> void:
	var h: Object = Engine.get_meta("gdt_harness")
	h.expect(1 + 1 == 2, "basic arithmetic")
	h.expect("gd" + "test" == "gdtest", "string concat")

## @tag harness-meta
func test_arithmetic_fails() -> void:
	var h: Object = Engine.get_meta("gdt_harness")
	h.expect(2 + 2 == 5, "expected 2+2 == 5")
	h.expect(true, "this one passes")
	h.expect(1 > 2, "1 > 2 is false")

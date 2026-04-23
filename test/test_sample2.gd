extends RefCounted

# Second sample file — exercises the pool with multi-file dispatch.

func test_multiplication() -> void:
	var h: Object = Engine.get_meta("gdt_harness")
	h.expect(3 * 4 == 12, "3 × 4 = 12")

func test_divmod() -> void:
	var h: Object = Engine.get_meta("gdt_harness")
	h.expect(10 % 3 == 1, "10 mod 3 = 1")
	h.expect(10 / 3 == 3, "10 div 3 = 3")

func test_arrays() -> void:
	var h: Object = Engine.get_meta("gdt_harness")
	var arr: Array[int] = [1, 2, 3]
	h.expect(arr.size() == 3, "array length")
	h.expect(arr.max() == 3, "array max")

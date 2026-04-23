extends GdTest

# ── Addition ─────────────────────────────────────────────────────────────────

func get_test_add_cases() -> Array:
	return [
		["adds_zero", [0, 0, 0]],
		["adds_positive", [2, 3, 5]],
		["adds_negative", [-1, 1, 0]],
		["adds_large", [100, 200, 300]],
	]

func test_add(a: int, b: int, expected: int) -> void:
	assert_eq(a + b, expected)

# ── String operations ─────────────────────────────────────────────────────────

func get_test_string_length_cases() -> Array:
	return [
		["empty", ["", 0]],
		["single", ["a", 1]],
		["word", ["hello", 5]],
		["multi_word", ["hello world", 11]],
	]

func test_string_length(s: String, expected: int) -> void:
	assert_eq(s.length(), expected)

# ── Float approx ──────────────────────────────────────────────────────────────

func get_test_approx_cases() -> Array:
	return [
		["zero_plus_zero", [0.0, 0.0, 1e-9]],
		["pi_approx", [3.14159, 3.14159, 1e-4]],
		["small_diff", [1.0, 1.0 + 1e-8, 1e-6]],
	]

func test_approx(a: float, b: float, tol: float) -> void:
	assert_almost_eq(a, b, tol)

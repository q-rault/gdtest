extends GdTest

# ── assert_push_error ─────────────────────────────────────────────────────────

func test_push_error_captured() -> void:
	Gdt.push_error("Cannot add null item")
	assert_push_error("Cannot add null item")

func test_push_error_missing_fails() -> void:
	assert_false(Gdt._check_push_error("non-existent error"))

func test_push_error_regex_match() -> void:
	Gdt.push_error("Player HP dropped to -5")
	assert_push_error("/HP dropped to -\\d+/")

func test_push_error_regex_no_match() -> void:
	Gdt.push_error("Player HP dropped to -5")
	assert_false(Gdt._check_push_error("/mana dropped/"))

func test_multiple_errors_any_match() -> void:
	Gdt.push_error("error one")
	Gdt.push_error("error two")
	assert_push_error("error two")

# ── assert_push_warning ───────────────────────────────────────────────────────

func test_push_warning_captured() -> void:
	Gdt.push_warning("Damage clamped to 0")
	assert_push_warning("Damage clamped to 0")

func test_push_warning_missing_fails() -> void:
	assert_false(Gdt._check_push_error("not captured"))

func test_push_warning_regex_match() -> void:
	Gdt.push_warning("Negative damage value clamped to 0")
	assert_push_warning("/clamped to \\d+/")

# ── assert_no_push_error ──────────────────────────────────────────────────────

func test_no_push_error_passes_when_clean() -> void:
	assert_no_push_error()

func test_no_push_error_fails_when_error_pushed() -> void:
	Gdt.push_error("some error")
	assert_false(Gdt._check_no_push_error())

# ── captures cleared between tests ───────────────────────────────────────────

func test_captures_cleared_a() -> void:
	Gdt.push_error("from test A — should not bleed into next test")

func test_captures_cleared_b() -> void:
	assert_no_push_error()

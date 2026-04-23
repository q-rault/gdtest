extends GdTest

# ── wait_until ────────────────────────────────────────────────────────────────

func test_wait_until_returns_immediately_when_condition_already_true() -> void:
	var calls := [0]
	await wait_until(func(): calls[0] += 1; return true, 500)
	assert_eq(calls[0], 1)

func test_wait_until_polls_until_condition_met() -> void:
	var state := [0]
	await wait_until(func(): state[0] += 1; return state[0] >= 3, 500)
	assert_gte(state[0], 3)

func test_wait_until_default_timeout_parameter_accepted() -> void:
	await wait_until(func(): return true)
	assert_true(true)

# ── wait_while ────────────────────────────────────────────────────────────────

func test_wait_while_returns_immediately_when_condition_already_false() -> void:
	var calls := [0]
	await wait_while(func(): calls[0] += 1; return false, 500)
	assert_eq(calls[0], 1)

func test_wait_while_polls_until_condition_becomes_false() -> void:
	var state := [0]
	await wait_while(func(): state[0] += 1; return state[0] < 3, 500)
	assert_gte(state[0], 3)

func test_wait_while_default_timeout_parameter_accepted() -> void:
	await wait_while(func(): return false)
	assert_true(true)

# ── interop with existing async helpers ───────────────────────────────────────

func test_wait_until_after_physics_frames() -> void:
	await wait_physics_frames(2)
	var done := [false]
	await wait_until(func(): done[0] = true; return done[0], 200)
	assert_true(done[0])

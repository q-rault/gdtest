extends GdTest

class DeltaRecorder extends Node:
	var last_delta: float = -1.0
	func _physics_process(delta: float) -> void:
		last_delta = delta

# ── set_physics_delta ─────────────────────────────────────────────────────────

func test_set_physics_delta_changes_tps() -> void:
	set_physics_delta(1.0 / 30.0)
	assert_eq(Engine.physics_ticks_per_second, 30)

func test_set_physics_delta_affects_physics_process_delta() -> void:
	set_physics_delta(1.0 / 30.0)
	var rec := DeltaRecorder.new()
	add_child_autofree(rec)
	await wait_physics_frames(2)
	assert_approx(rec.last_delta, 1.0 / 30.0, 1e-4)

func test_set_physics_delta_60hz() -> void:
	set_physics_delta(1.0 / 60.0)
	assert_eq(Engine.physics_ticks_per_second, 60)

# ── reset_physics_delta ───────────────────────────────────────────────────────

func test_reset_restores_original_tps() -> void:
	var original := Engine.physics_ticks_per_second
	set_physics_delta(1.0 / 30.0)
	reset_physics_delta()
	assert_eq(Engine.physics_ticks_per_second, original)

func test_reset_without_prior_set_is_a_noop() -> void:
	var original := Engine.physics_ticks_per_second
	reset_physics_delta()
	assert_eq(Engine.physics_ticks_per_second, original)

# ── auto-reset after each test ────────────────────────────────────────────────

func test_auto_reset_part1_set_weird_value() -> void:
	set_physics_delta(1.0 / 15.0)
	assert_eq(Engine.physics_ticks_per_second, 15)
	# harness restores original tps after this test

func test_auto_reset_part2_tps_is_restored() -> void:
	# Expects the auto-reset from part1 to have fired.
	# Default project tps is 60; it should not be 15 here.
	assert_ne(Engine.physics_ticks_per_second, 15)

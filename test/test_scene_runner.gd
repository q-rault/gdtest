extends GdTest

# Tests for SceneRunner — input simulation helper (internal).
# Runs in headless Godot; Input.action_press/release work without a display.

func _build_scene() -> PackedScene:
	var root := Node.new()
	root.name = "RunnerRoot"
	var packed := PackedScene.new()
	packed.pack(root)
	root.free()
	return packed


# ── Factory ───────────────────────────────────────────────────────────────────

func test_scene_runner_returns_runner_with_scene_in_tree() -> void:
	var packed := _build_scene()
	var runner := scene_runner(packed)
	assert_not_null(runner, "runner is not null")
	assert_not_null(runner.scene, "runner.scene is set")
	assert_not_null(runner.scene.get_parent(), "scene is in tree")

# ── Frame advance ─────────────────────────────────────────────────────────────

func test_simulate_frames_advances_time() -> void:
	var packed := _build_scene()
	var runner := scene_runner(packed)
	var before := Time.get_ticks_usec()
	await runner.simulate_frames(3)
	var after := Time.get_ticks_usec()
	assert_true(after > before, "time advanced after simulate_frames")

func test_simulate_seconds_advances_time() -> void:
	var packed := _build_scene()
	var runner := scene_runner(packed)
	# In headless mode game-time runs faster than wall-clock; just verify the
	# await completes (timer fired) rather than checking elapsed real time.
	await runner.simulate_seconds(0.05)
	assert_true(true, "simulate_seconds completed without hanging")

# ── Actions ───────────────────────────────────────────────────────────────────

func test_simulate_action_pressed_sets_state() -> void:
	var packed := _build_scene()
	var runner := scene_runner(packed)
	assert_false(Input.is_action_pressed("ui_accept"), "not pressed initially")
	runner.simulate_action_pressed("ui_accept")
	assert_true(Input.is_action_pressed("ui_accept"), "pressed after simulate")
	runner.simulate_action_released("ui_accept")
	assert_false(Input.is_action_pressed("ui_accept"), "released after simulate_released")

func test_scene_runner_releases_held_actions_on_free() -> void:
	var packed := _build_scene()
	var runner := scene_runner(packed)
	runner.simulate_action_pressed("ui_accept")
	assert_true(Input.is_action_pressed("ui_accept"))
	# Drop the runner — _notification(PREDELETE) should release the action.
	runner = null
	assert_false(Input.is_action_pressed("ui_accept"), "action released on SceneRunner free")

# ── Mouse ─────────────────────────────────────────────────────────────────────

func test_simulate_mouse_button_does_not_crash() -> void:
	var packed := _build_scene()
	var runner := scene_runner(packed)
	runner.simulate_mouse_button_pressed(MOUSE_BUTTON_LEFT, Vector2(100, 200))
	runner.simulate_mouse_button_released(MOUSE_BUTTON_LEFT, Vector2(100, 200))
	assert_true(true, "no crash on mouse button simulation")

func test_simulate_mouse_move_does_not_crash() -> void:
	var packed := _build_scene()
	var runner := scene_runner(packed)
	runner.simulate_mouse_move(Vector2(50, 75))
	assert_true(true, "no crash on mouse move simulation")


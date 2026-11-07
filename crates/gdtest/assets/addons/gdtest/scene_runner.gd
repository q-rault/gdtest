class_name SceneRunner
extends RefCounted

# SceneRunner — input simulation helper.
#
# Obtain via `scene_runner(packed)` from a GdTest (or Gdt.scene_runner()).
# The scene is already instantiated and attached to the test container.
#
# Usage:
#   var runner := scene_runner(load("res://player.tscn"))
#   runner.simulate_action_pressed("jump")
#   await runner.simulate_frames(3)
#   assert_eq(runner.scene.state, "jumping")

var scene: Node

# Tracks actions pressed by this runner so they're released on cleanup.
var _held_actions: Array[String] = []

func _init(node: Node) -> void:
	scene = node

func _notification(what: int) -> void:
	if what == NOTIFICATION_PREDELETE:
		for action in _held_actions:
			Input.action_release(action)
		_held_actions.clear()

# ── Frame / time advance ──────────────────────────────────────────────────────

func simulate_frames(count: int = 1) -> void:
	for _i in count:
		await Engine.get_main_loop().process_frame

func simulate_seconds(duration: float) -> void:
	var timer := (Engine.get_main_loop() as SceneTree).create_timer(duration)
	await timer.timeout

# ── Keyboard ──────────────────────────────────────────────────────────────────

func simulate_key_pressed(key: Key, modifiers: int = 0) -> void:
	var ev := InputEventKey.new()
	ev.keycode = key
	ev.pressed = true
	ev.ctrl_pressed = (modifiers & KEY_MASK_CTRL) != 0
	ev.shift_pressed = (modifiers & KEY_MASK_SHIFT) != 0
	ev.alt_pressed = (modifiers & KEY_MASK_ALT) != 0
	Input.parse_input_event(ev)

func simulate_key_released(key: Key) -> void:
	var ev := InputEventKey.new()
	ev.keycode = key
	ev.pressed = false
	Input.parse_input_event(ev)

# ── Actions ───────────────────────────────────────────────────────────────────
#
# Uses Input.action_press / action_release so that Input.is_action_pressed()
# reflects the simulated state immediately. Also dispatches an InputEventAction
# for nodes that handle input via _input() / _unhandled_input().

func simulate_action_pressed(action: String) -> void:
	Input.action_press(action)
	if action not in _held_actions:
		_held_actions.append(action)
	var ev := InputEventAction.new()
	ev.action = action
	ev.pressed = true
	Input.parse_input_event(ev)

func simulate_action_released(action: String) -> void:
	Input.action_release(action)
	_held_actions.erase(action)
	var ev := InputEventAction.new()
	ev.action = action
	ev.pressed = false
	Input.parse_input_event(ev)

# ── Mouse ─────────────────────────────────────────────────────────────────────

func simulate_mouse_move(position: Vector2) -> void:
	var ev := InputEventMouseMotion.new()
	ev.position = position
	ev.global_position = position
	Input.parse_input_event(ev)

func simulate_mouse_button_pressed(button: MouseButton, position: Vector2 = Vector2.ZERO) -> void:
	var ev := InputEventMouseButton.new()
	ev.button_index = button
	ev.pressed = true
	ev.position = position
	ev.global_position = position
	Input.parse_input_event(ev)

func simulate_mouse_button_released(button: MouseButton, position: Vector2 = Vector2.ZERO) -> void:
	var ev := InputEventMouseButton.new()
	ev.button_index = button
	ev.pressed = false
	ev.position = position
	ev.global_position = position
	Input.parse_input_event(ev)

# Simulates a full left-click on a GUI Control (hover → press → release).
# Uses viewport-local coordinates from get_global_rect() and push_input() with
# in_local_coords=true, which is required for correct routing in windowed mode.
# Requires --display-mode windowed; skip tests calling this in headless runs.
func simulate_click_control(control: Control) -> void:
	var vp := control.get_viewport()
	# get_global_transform_with_canvas() maps from the control's LOCAL space to
	# viewport space, including any CanvasLayer transform. The center in local
	# space is size/2. Using get_global_rect().get_center() is wrong for controls
	# inside a CanvasLayer because it double-counts the control's own position.
	var center := control.get_global_transform_with_canvas() * (control.size / 2.0)

	var motion := InputEventMouseMotion.new()
	motion.position = center
	motion.global_position = center
	vp.push_input(motion, true)

	await Engine.get_main_loop().process_frame

	var press := InputEventMouseButton.new()
	press.button_index = MOUSE_BUTTON_LEFT
	press.pressed = true
	press.position = center
	press.global_position = center
	vp.push_input(press, true)

	var release := InputEventMouseButton.new()
	release.button_index = MOUSE_BUTTON_LEFT
	release.pressed = false
	release.position = center
	release.global_position = center
	vp.push_input(release, true)

	await Engine.get_main_loop().process_frame

extends GdTest

# GDScript 4 lambdas capture primitive types (bool, int) by VALUE — reassigning
# inside a lambda does NOT update the outer variable. Use a Dictionary or Array
# so mutations propagate back through the shared reference.

func _make_motion(pos: Vector2) -> InputEventMouseMotion:
	var ev := InputEventMouseMotion.new()
	ev.position = pos; ev.global_position = pos
	return ev

func _make_press(pos: Vector2) -> InputEventMouseButton:
	var ev := InputEventMouseButton.new()
	ev.button_index = MOUSE_BUTTON_LEFT; ev.pressed = true
	ev.position = pos; ev.global_position = pos
	return ev

func _make_release(pos: Vector2) -> InputEventMouseButton:
	var ev := InputEventMouseButton.new()
	ev.button_index = MOUSE_BUTTON_LEFT; ev.pressed = false
	ev.position = pos; ev.global_position = pos
	return ev

func _push_click(vp: Viewport, center: Vector2) -> void:
	vp.push_input(_make_motion(center), true)
	vp.push_input(_make_press(center), true)
	vp.push_input(_make_release(center), true)

func _make_button() -> Button:
	var b := Button.new()
	b.text = "Test"
	b.size = Vector2(150.0, 50.0)
	b.position = Vector2(50.0, 50.0)
	add_child_autofree(b)
	return b


# ── Test A : click de base (push_input in_local_coords=true) ─────────────────

func test_A_click_fires_pressed() -> void:
	if OS.has_feature("headless"):
		skip("require --display-mode windowed")
		return

	var button := _make_button()
	var s := {"pressed": false, "down": false, "up": false, "hovered_at_press": false}
	button.pressed.connect(func(): s.pressed = true)
	button.button_down.connect(func(): s.down = true)
	button.button_up.connect(func(): s.up = true)
	button.gui_input.connect(func(e):
		if e is InputEventMouseButton and e.pressed:
			s.hovered_at_press = button.is_hovered())

	for _i in 3: await wait_process_frame()

	var vp := button.get_viewport()
	var center := button.get_global_rect().get_center()
	_push_click(vp, center)
	await wait_process_frame()

	var info := "pressed=%s down=%s up=%s hovered_at_press=%s is_hovered=%s" % [
		str(s.pressed), str(s.down), str(s.up), str(s.hovered_at_press), str(button.is_hovered())
	]
	assert_true(s.down, "A: button_down doit fire (%s)" % info)
	assert_true(s.up, "A: button_up doit fire (%s)" % info)
	assert_true(s.pressed, "A: pressed doit fire (%s)" % info)


# ── Test B : keep_pressed_outside=true ───────────────────────────────────────

func test_B_keep_pressed_outside() -> void:
	if OS.has_feature("headless"):
		skip("require --display-mode windowed")
		return

	var button := _make_button()
	button.keep_pressed_outside = true
	var s := {"pressed": false, "down": false}
	button.pressed.connect(func(): s.pressed = true)
	button.button_down.connect(func(): s.down = true)

	for _i in 3: await wait_process_frame()

	var vp := button.get_viewport()
	var center := button.get_global_rect().get_center()
	_push_click(vp, center)
	await wait_process_frame()

	assert_true(s.down,
		"B: button_down avec keep_pressed_outside (down=%s pressed=%s)" % [str(s.down), str(s.pressed)])
	assert_true(s.pressed,
		"B: pressed avec keep_pressed_outside (down=%s pressed=%s)" % [str(s.down), str(s.pressed)])


# ── Test C : motion puis press séparés avec await ────────────────────────────

func test_C_motion_then_press_with_await() -> void:
	if OS.has_feature("headless"):
		skip("require --display-mode windowed")
		return

	var button := _make_button()
	var s := {"pressed": false, "down": false, "hovered_at_press": false}
	button.pressed.connect(func(): s.pressed = true)
	button.button_down.connect(func(): s.down = true)
	button.gui_input.connect(func(e):
		if e is InputEventMouseButton and e.pressed:
			s.hovered_at_press = button.is_hovered())

	for _i in 3: await wait_process_frame()

	var vp := button.get_viewport()
	var center := button.get_global_rect().get_center()

	vp.push_input(_make_motion(center), true)
	await wait_process_frame()
	var hovered_before := button.is_hovered()
	vp.push_input(_make_press(center), true)
	vp.push_input(_make_release(center), true)
	await wait_process_frame()

	var info := "hovered_before=%s hovered_at_press=%s down=%s pressed=%s" % [
		str(hovered_before), str(s.hovered_at_press), str(s.down), str(s.pressed)
	]
	assert_true(hovered_before, "C: is_hovered() doit être true avant press (%s)" % info)
	assert_true(s.down, "C: button_down (%s)" % info)
	assert_true(s.pressed, "C: pressed (%s)" % info)

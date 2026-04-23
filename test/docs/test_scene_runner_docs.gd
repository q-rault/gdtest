## Living docs for scene-runner.mdx — regions embedded verbatim in the page.
extends GdTest

# region: pickup
func test_potion_pickup() -> void:
	var runner := scene_runner(preload("res://test/docs/dungeon/health_potion.tscn"))
	var potion := runner.scene as HealthPotion
	var h := Health.new(100)
	h.take_damage(20)  # hp = 80
	var healed: Array[int] = [0]
	potion.picked_up.connect(func(amount: int) -> void: healed[0] = amount)
	potion.collect(h)
	assert_eq(h.hp, 100)
	assert_eq(healed[0], 20)
# endregion

# region: frames
func test_frame_advance() -> void:
	var runner := scene_runner(preload("res://test/docs/dungeon/health_potion.tscn"))
	var ticks: Array[int] = [0]
	Engine.get_main_loop().process_frame.connect(func() -> void: ticks[0] += 1, CONNECT_ONE_SHOT)
	await runner.simulate_frames(3)
	assert_gte(ticks[0], 1)
# endregion

# region: action
func test_action_simulation() -> void:
	var runner := scene_runner(preload("res://test/docs/dungeon/health_potion.tscn"))
	InputMap.add_action("jump")
	runner.simulate_action_pressed("jump")
	assert_true(Input.is_action_pressed("jump"))
	runner.simulate_action_released("jump")
	assert_false(Input.is_action_pressed("jump"))
	InputMap.erase_action("jump")
# endregion

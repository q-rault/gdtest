extends GdTest

# ── world creation ────────────────────────────────────────────────────────────

func test_world_is_non_null() -> void:
	var world := test_world()
	assert_not_null(world)

func test_world_container_is_in_tree() -> void:
	var world := test_world()
	world.spawn(Node2D)
	# Container is auto-freed after test — no orphan
	assert_no_new_orphans()

# ── spawn + ActorProxy ────────────────────────────────────────────────────────

func test_spawn_returns_actor_proxy() -> void:
	var world := test_world()
	var actor = world.spawn(Node2D)
	assert_not_null(actor)
	assert_not_null(actor.instance)

func test_spawn_instance_is_in_tree() -> void:
	var world := test_world()
	var actor = world.spawn(Node2D)
	assert_true((actor.instance as Node2D).is_inside_tree())

func test_spawn_multiple_actors() -> void:
	var world := test_world()
	var a = world.spawn(Node2D)
	var b = world.spawn(Node2D)
	assert_ne(a.instance.get_instance_id(), b.instance.get_instance_id())

# ── at() fluent placement ─────────────────────────────────────────────────────

func test_at_sets_position_2d() -> void:
	var world := test_world()
	var actor = world.spawn(Node2D).at(Vector2(150.0, 75.0))
	assert_eq((actor.instance as Node2D).position, Vector2(150.0, 75.0))

func test_at_returns_actor_proxy_for_chaining() -> void:
	var world := test_world()
	var actor = world.spawn(Node2D).at(Vector2(10.0, 20.0))
	assert_not_null(actor)
	assert_eq((actor.instance as Node2D).position, Vector2(10.0, 20.0))

# ── physics_frames / process_frames / ready ───────────────────────────────────

func test_physics_frames_is_awaitable() -> void:
	var world := test_world()
	await world.physics_frames(2)
	assert_true(true)

func test_process_frames_is_awaitable() -> void:
	var world := test_world()
	await world.process_frames(2)
	assert_true(true)

func test_ready_is_awaitable() -> void:
	var world := test_world()
	await world.ready()
	assert_true(true)

# ── auto-cleanup — no orphans after world use ─────────────────────────────────

func test_world_nodes_freed_after_test_a() -> void:
	var world := test_world()
	world.spawn(Node2D)
	world.spawn(Node2D)

func test_world_nodes_freed_after_test_b() -> void:
	assert_no_new_orphans()

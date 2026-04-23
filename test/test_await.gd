extends RefCounted

# Exercises internal await support.

func test_sync_still_works() -> void:
	var h: Object = Engine.get_meta("gdt_harness")
	h.expect(1 + 1 == 2, "sync arithmetic")

func test_direct_await_physics_frame() -> void:
	var h: Object = Engine.get_meta("gdt_harness")
	var tree: SceneTree = h
	var before: int = Engine.get_physics_frames()
	await tree.physics_frame
	var after: int = Engine.get_physics_frames()
	h.expect(after - before == 1, "1 physics_frame advance (got %d)" % (after - before))

func test_simulate_helper() -> void:
	var h: Object = Engine.get_meta("gdt_harness")
	var before: int = Engine.get_physics_frames()
	await h.simulate_physics_frames(3)
	var after: int = Engine.get_physics_frames()
	h.expect(after - before == 3, "3 simulate_physics_frames ⇒ 3 ticks (got %d)" % (after - before))

func test_simulate_seconds() -> void:
	var h: Object = Engine.get_meta("gdt_harness")
	var before: int = Engine.get_physics_frames()
	await h.simulate_physics_seconds(0.05)
	var after: int = Engine.get_physics_frames()
	# 60 Hz * 0.05s = 3 ticks
	h.expect(after - before >= 3, "0.05s ⇒ ≥ 3 ticks (got %d)" % (after - before))

## @tag harness-meta
func test_post_await_failure_surfaces() -> void:
	# Regression guard: a failure recorded AFTER an await must be captured.
	var h: Object = Engine.get_meta("gdt_harness")
	await h.simulate_physics_frames(1)
	h.expect(false, "deliberate failure after await")

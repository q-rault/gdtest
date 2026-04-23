extends GdTest

# Exercises scene helpers from a GdTest (RefCounted) test.
# Tests don't extend Node, but they need to instantiate scenes and Nodes
# into the tree.

const _CHILD_SCRIPT: GDScript = preload("res://test/_scene_helpers_child.gd")

func test_add_child_autofree_returns_the_node_in_tree() -> void:
	var n := Node.new()
	n.name = "child_in_tree"
	var returned := add_child_autofree(n)
	assert_true(returned == n, "returned the same node for chaining")
	assert_not_null(n.get_parent(), "node has a parent after add_child_autofree")

func test_instantiate_scene_attaches_under_container() -> void:
	var packed := _build_inline_scene()
	var inst := instantiate_scene(packed)
	assert_not_null(inst, "instance returned")
	var inner := inst.get_node("Inner")
	assert_not_null(inner, "child node reachable via get_node")
	assert_true(bool(inner.ready_fired), "_ready ran during instantiate_scene")

func test_async_works_after_instantiate() -> void:
	var packed := _build_inline_scene()
	var inst := instantiate_scene(packed)
	await wait_process_frame()
	assert_not_null(inst.get_node("Inner"), "scene survives an await mid-test")

func test_refcounted_test_does_not_create_container() -> void:
	# Container is created lazily on the first add_child_autofree call.
	# A test that never reaches that helper pays zero tree overhead.
	var tree: SceneTree = Engine.get_main_loop()
	var container := tree.root.find_child("_GdTest", false, false)
	assert_null(container, "no container created for tests that don't use add_child_autofree")

# ── helpers ──────────────────────────────────────────────────────────────

func _build_inline_scene() -> PackedScene:
	var root_node := Node.new()
	root_node.name = "InlineRoot"
	var inner := Node.new()
	inner.name = "Inner"
	inner.set_script(_CHILD_SCRIPT)
	root_node.add_child(inner)
	inner.owner = root_node
	var packed := PackedScene.new()
	packed.pack(root_node)
	root_node.free()
	return packed

extends GdTest

# ── passing cases ─────────────────────────────────────────────────────────────

func test_no_orphans_when_nothing_created() -> void:
	assert_no_new_orphans()

func test_no_orphans_when_node_explicitly_freed() -> void:
	var n := Node.new()
	n.free()
	assert_no_new_orphans()

func test_no_orphans_when_node_added_to_tree_via_autofree() -> void:
	add_child_autofree(Node.new())
	assert_no_new_orphans()

func test_no_orphans_after_multiple_alloc_free_cycles() -> void:
	for i in 5:
		var n := Node.new()
		n.free()
	assert_no_new_orphans()

# ── orphan detection via Performance monitor (verifies the mechanism) ─────────

func test_orphan_monitor_increases_for_leaked_node() -> void:
	var before: int = int(Performance.get_monitor(Performance.OBJECT_ORPHAN_NODE_COUNT))
	var n := Node.new()
	var after: int = int(Performance.get_monitor(Performance.OBJECT_ORPHAN_NODE_COUNT))
	assert_gt(after, before)
	n.free()  # clean up so this test leaves no orphans itself

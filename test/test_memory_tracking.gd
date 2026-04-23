extends GdTest

# ── autoref — Node ────────────────────────────────────────────────────────────

func test_autoref_returns_node() -> void:
	var n := autoref(Node.new()) as Node
	assert_not_null(n)
	assert_true(is_instance_valid(n))

func test_autoref_node_valid_during_test() -> void:
	var n := autoref(Node.new())
	assert_true(is_instance_valid(n))
	# n freed automatically after test → no orphan in the next baseline

# ── autoref — Resource (RefCounted) ──────────────────────────────────────────

func test_autoref_resource_returns_resource() -> void:
	var r := autoref(Resource.new()) as Resource
	assert_not_null(r)
	assert_true(is_instance_valid(r))

func test_autoref_resource_valid_during_test() -> void:
	var r := autoref(Resource.new())
	assert_is(r, Resource)

# ── add_child_autofree ────────────────────────────────────────────────────────

func test_add_child_autofree_node_is_in_tree() -> void:
	var n := add_child_autofree(Node.new())
	assert_true(n.is_inside_tree())
	assert_no_new_orphans()

# ── heap delta ────────────────────────────────────────────────────────────────

func test_heap_delta_is_numeric() -> void:
	var delta := current_heap_delta()
	assert_true(typeof(delta) == TYPE_INT)

func test_heap_delta_reasonable_after_small_alloc() -> void:
	# Allocate a large array to ensure delta is trackable in principle.
	# We don't assert an exact value — GC timing makes that fragile.
	# Just assert the API returns something plausible.
	var _big := [].duplicate()
	for i in 1000:
		_big.append(i)
	var delta := current_heap_delta()
	assert_true(delta > -1024 * 1024)  # no more than 1 MB negative (sane range)

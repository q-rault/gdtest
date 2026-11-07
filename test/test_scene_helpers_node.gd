extends Node

# Test class that extends Node directly — verifies the branch
# where the harness adds the test instance itself to the per-test
# container so its `_ready()` runs.

var _ready_called: bool = false
var _process_seen: bool = false

func _ready() -> void:
	_ready_called = true

func _process(_delta: float) -> void:
	_process_seen = true

func test_ready_fired_before_test() -> void:
	Gdt.expect_true(_ready_called, "_ready ran when the test instance was added to the tree")

func test_get_tree_returns_the_harness_tree() -> void:
	var tree := get_tree()
	Gdt.expect_not_null(tree, "test instance has access to the SceneTree")
	# The harness IS a SceneTree, so get_tree() must equal it.
	Gdt.expect_true(tree == Engine.get_main_loop(), "get_tree() == Engine.get_main_loop()")

func test_process_runs_after_a_frame() -> void:
	# _process won't have run synchronously on add_child; advance frames
	# until we observe the tick. In headless mode _process can fire
	# before the test even runs (between add_child and the test method
	# call) or take a couple ticks to be observed depending on signal
	# vs callback ordering — wait up to 4 frames before declaring fail.
	for i in 4:
		if _process_seen:
			break
		await Gdt.wait_process_frame()
	Gdt.expect_true(_process_seen, "_process ran within 4 frames of being in the tree")

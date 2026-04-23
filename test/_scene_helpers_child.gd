extends Node

# Tiny child script used by test_scene_helpers.gd's inline PackedScene.
# Files starting with `_` aren't matched by gdtest's `test_*.gd` glob, so
# it sits next to the tests without being treated as a test file itself.

var ready_fired: bool = false

func _ready() -> void:
	ready_fired = true

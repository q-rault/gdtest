extends GdTest

# Tests for assert_snapshot (internal).
# Snapshots are written to test/__snapshots__/test_snapshot/ on first run,
# then compared on every subsequent run. Delete the directory to reset.

func test_snapshot_int() -> void:
	assert_snapshot(42, "int_value")

func test_snapshot_string() -> void:
	assert_snapshot("hello world", "string_value")

func test_snapshot_array() -> void:
	assert_snapshot([1, 2, 3], "array_value")

func test_snapshot_dict() -> void:
	assert_snapshot({"hp": 100, "name": "player"}, "dict_value")

func test_snapshot_auto_name_increments() -> void:
	assert_snapshot(10, "")  # stored as key "0"
	assert_snapshot(20, "")  # stored as key "1"

func test_snapshot_second_run_matches() -> void:
	# Forces a read path: if the snapshot was created by a prior test_snapshot_int
	# run in the same suite, this verifies comparison works too.
	var snap_path := "res://test/__snapshots__/test_snapshot/int_value.gdsnap"
	if FileAccess.file_exists(snap_path):
		assert_snapshot(42, "int_value")
	else:
		pending("snapshot file not yet created — run the suite once first")

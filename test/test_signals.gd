extends GdTest

signal my_signal()
signal my_data_signal(value: int)

# Deferred emitters — start without await so the watcher connects first.

func _emit_next_frame() -> void:
	await wait_process_frame()
	my_signal.emit()

func _emit_n_next_frame(n: int) -> void:
	await wait_process_frame()
	for i in n:
		my_signal.emit()

func _emit_data_next_frame(value: int) -> void:
	await wait_process_frame()
	my_data_signal.emit(value)

# ── Tests ─────────────────────────────────────────────────────────────────────

func test_signal_fires() -> void:
	_emit_next_frame()
	await assert_signal(self, "my_signal", 500)

func test_no_signal_passes_when_silent() -> void:
	await assert_no_signal(self, "my_signal", 100)

func test_signal_count_two() -> void:
	_emit_n_next_frame(2)
	await assert_signal_count(self, "my_signal", 2, 500)

func test_signal_with_args() -> void:
	_emit_data_next_frame(42)
	await assert_signal_with_args(self, "my_data_signal", [42], 500)

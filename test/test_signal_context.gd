extends GdTest

signal my_signal()

func _emit_next_frame() -> void:
	await wait_process_frame()
	my_signal.emit()

# context_fn is accepted without error; it is only invoked on timeout, not on success.
func test_assert_signal_accepts_context_fn() -> void:
	var ctx_called := [false]
	_emit_next_frame()
	await assert_signal(self, "my_signal", 200, func(): ctx_called[0] = true; return {"state": "ok"})
	assert_false(ctx_called[0])

# assert_signal without context_fn still works (regression).
func test_assert_signal_without_context_fn() -> void:
	_emit_next_frame()
	await assert_signal(self, "my_signal", 200)
	assert_true(true)

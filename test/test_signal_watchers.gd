extends GdTest

const SignalSource = preload("support/signal_source.gd")

# ── count ─────────────────────────────────────────────────────────────────────

func test_watch_count_starts_at_zero() -> void:
	var src := SignalSource.new()
	var w = watch_signal(src, "pinged")
	assert_eq(w.count, 0)

func test_watch_count_increments_per_emission() -> void:
	var src := SignalSource.new()
	var w = watch_signal(src, "pinged")
	src.ping()
	src.ping()
	assert_eq(w.count, 2)

# ── emissions ─────────────────────────────────────────────────────────────────

func test_watch_zero_arg_emission_stored_as_empty_array() -> void:
	var src := SignalSource.new()
	var w = watch_signal(src, "pinged")
	src.ping()
	assert_eq(w.emissions.size(), 1)
	assert_eq(w.emissions[0], [])

func test_watch_single_arg_emission() -> void:
	var src := SignalSource.new()
	var w = watch_signal(src, "hit")
	src.take_hit(42)
	assert_eq(w.emissions[0][0], 42)

func test_watch_multi_arg_emission() -> void:
	var src := SignalSource.new()
	var w = watch_signal(src, "moved")
	src.move(10.0, 20.0)
	assert_eq(w.emissions[0][0], 10.0)
	assert_eq(w.emissions[0][1], 20.0)

func test_watch_emission_history() -> void:
	var src := SignalSource.new()
	var w = watch_signal(src, "hit")
	src.take_hit(10)
	src.take_hit(20)
	assert_eq(w.emissions[0][0], 10)
	assert_eq(w.emissions[1][0], 20)

# ── last_args ─────────────────────────────────────────────────────────────────

func test_watch_last_args_empty_before_emission() -> void:
	var src := SignalSource.new()
	var w = watch_signal(src, "hit")
	assert_eq(w.last_args, [])

func test_watch_last_args_reflects_latest_emission() -> void:
	var src := SignalSource.new()
	var w = watch_signal(src, "hit")
	src.take_hit(10)
	src.take_hit(99)
	assert_eq(w.last_args[0], 99)

# ── reset ─────────────────────────────────────────────────────────────────────

func test_watch_reset_clears_emissions() -> void:
	var src := SignalSource.new()
	var w = watch_signal(src, "pinged")
	src.ping()
	src.ping()
	w.reset()
	assert_eq(w.count, 0)
	assert_true(w.emissions.is_empty())

func test_watch_still_records_after_reset() -> void:
	var src := SignalSource.new()
	var w = watch_signal(src, "pinged")
	src.ping()
	w.reset()
	src.ping()
	assert_eq(w.count, 1)

# ── error handling ────────────────────────────────────────────────────────────

func test_watch_invalid_signal_returns_null() -> void:
	var src := SignalSource.new()
	var w = Gdt.watch_signal(src, "no_such_signal")
	assert_null(w)

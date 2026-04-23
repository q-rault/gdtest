extends GdTest

const SignalSource = preload("support/signal_source.gd")

# ── assert_emitted_before ─────────────────────────────────────────────────────

func test_emitted_before_passes_on_correct_order() -> void:
	var src := SignalSource.new()
	watch_signal(src, "pinged")
	watch_signal(src, "hit")
	src.ping()
	src.take_hit(1)
	assert_emitted_before(src, "pinged", src, "hit")

func test_emitted_before_fails_on_reversed_order() -> void:
	var src := SignalSource.new()
	watch_signal(src, "pinged")
	watch_signal(src, "hit")
	src.take_hit(1)
	src.ping()
	assert_false(Gdt._check_emitted_before(src, "pinged", src, "hit"))

func test_emitted_before_across_different_objects() -> void:
	var a := SignalSource.new()
	var b := SignalSource.new()
	watch_signal(a, "pinged")
	watch_signal(b, "pinged")
	a.ping()
	b.ping()
	assert_emitted_before(a, "pinged", b, "pinged")

func test_emitted_before_same_signal_reversed_objects() -> void:
	var a := SignalSource.new()
	var b := SignalSource.new()
	watch_signal(a, "pinged")
	watch_signal(b, "pinged")
	b.ping()
	a.ping()
	assert_false(Gdt._check_emitted_before(a, "pinged", b, "pinged"))

# ── assert_signal_order ───────────────────────────────────────────────────────

func test_signal_order_passes_on_correct_sequence() -> void:
	var src := SignalSource.new()
	watch_signal(src, "pinged")
	watch_signal(src, "hit")
	watch_signal(src, "moved")
	src.ping()
	src.take_hit(1)
	src.move(1.0, 2.0)
	assert_signal_order([
		{"obj": src, "sig": "pinged"},
		{"obj": src, "sig": "hit"},
		{"obj": src, "sig": "moved"},
	])

func test_signal_order_fails_on_wrong_sequence() -> void:
	var src := SignalSource.new()
	watch_signal(src, "pinged")
	watch_signal(src, "hit")
	src.take_hit(1)
	src.ping()
	assert_false(Gdt._check_signal_order([
		{"obj": src, "sig": "pinged"},
		{"obj": src, "sig": "hit"},
	]))

func test_signal_order_across_objects() -> void:
	var a := SignalSource.new()
	var b := SignalSource.new()
	var c := SignalSource.new()
	watch_signal(a, "pinged")
	watch_signal(b, "hit")
	watch_signal(c, "moved")
	a.ping()
	b.take_hit(5)
	c.move(0.0, 0.0)
	assert_signal_order([
		{"obj": a, "sig": "pinged"},
		{"obj": b, "sig": "hit"},
		{"obj": c, "sig": "moved"},
	])

# ── edge cases ────────────────────────────────────────────────────────────────

func test_check_emitted_before_returns_false_for_missing_watcher() -> void:
	var src := SignalSource.new()
	assert_false(Gdt._check_emitted_before(src, "pinged", src, "hit"))

func test_check_emitted_before_returns_false_when_unemitted() -> void:
	var src := SignalSource.new()
	watch_signal(src, "pinged")
	watch_signal(src, "hit")
	assert_false(Gdt._check_emitted_before(src, "pinged", src, "hit"))

func test_emission_seqs_populated_on_watcher() -> void:
	var src := SignalSource.new()
	var w = watch_signal(src, "hit")
	src.take_hit(10)
	src.take_hit(20)
	assert_eq(w.emission_seqs.size(), 2)
	assert_true(w.emission_seqs[0] < w.emission_seqs[1])

func test_emissions_still_accessible_after_refactor() -> void:
	var src := SignalSource.new()
	var w = watch_signal(src, "hit")
	src.take_hit(42)
	assert_eq(w.emissions[0][0], 42)

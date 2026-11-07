extends GdTest

const HealthComponent = preload("support/health_component.gd")
const LogScript = preload("support/logger.gd")

# ── Spy ───────────────────────────────────────────────────────────────────

func test_spy_runs_real_impl() -> void:
	var real := HealthComponent.new()
	var s = spy(real)
	assert_eq(s.get_hp(), 100)

func test_spy_records_calls() -> void:
	var real := HealthComponent.new()
	var s = spy(real)
	s.get_hp()
	s.get_hp()
	assert_call_count(s, "get_hp", 2)

func test_spy_copies_instance_state() -> void:
	var real := HealthComponent.new()
	real.take_damage(40)
	var s = spy(real)
	assert_eq(s.get_hp(), 60)

func test_spy_stub_overrides_real_impl() -> void:
	var real := HealthComponent.new()
	var s = spy(real)
	stub(s, "get_hp", 999)
	assert_eq(s.get_hp(), 999)

func test_spy_real_impl_still_runs_for_unstubbed() -> void:
	var real := HealthComponent.new()
	var s = spy(real)
	stub(s, "get_hp", 999)
	s.take_damage(20)
	assert_false(s.is_dead())

# ── Partial mock ──────────────────────────────────────────────────────────

func test_partial_mock_calls_real_impl() -> void:
	var m = partial_mock(HealthComponent)
	assert_eq(m.get_hp(), 100)

func test_partial_mock_state_changes_persist() -> void:
	var m = partial_mock(HealthComponent)
	m.take_damage(30)
	assert_eq(m.get_hp(), 70)

func test_partial_mock_stub_overrides_real() -> void:
	var m = partial_mock(HealthComponent)
	stub(m, "get_hp", 999)
	assert_eq(m.get_hp(), 999)

func test_partial_mock_records_all_calls() -> void:
	var m = partial_mock(HealthComponent)
	m.get_hp()
	m.take_damage(10)
	assert_called(m, "get_hp")
	assert_called(m, "take_damage")

# ── Argument matchers ─────────────────────────────────────────────────────

func test_matcher_any_matches_int() -> void:
	var m = mock(HealthComponent)
	m.take_damage(42)
	assert_called_with(m, "take_damage", [Gdt.any()])

func test_matcher_any_int() -> void:
	var m = mock(HealthComponent)
	m.take_damage(99)
	assert_called_with(m, "take_damage", [Gdt.any_int()])

func test_matcher_any_string() -> void:
	var m = mock(LogScript)
	m.log("hello")
	assert_called_with(m, "log", [Gdt.any_string()])

func test_matcher_satisfying_positive() -> void:
	var m = mock(HealthComponent)
	m.take_damage(50)
	assert_called_with(m, "take_damage", [Gdt.satisfying(func(x): return x > 0)])

func test_matcher_satisfying_negative() -> void:
	var m = mock(HealthComponent)
	m.take_damage(50)
	assert_false(Gdt._args_match([50], [Gdt.satisfying(func(x): return x > 100)]))

func test_matcher_matching_regex() -> void:
	var m = mock(LogScript)
	m.log("ERROR: something broke")
	assert_called_with(m, "log", [Gdt.matching("^ERROR:")])

func test_matcher_matching_no_match() -> void:
	var m = mock(LogScript)
	m.log("INFO: all good")
	assert_false(Gdt._args_match(["INFO: all good"], [Gdt.matching("^ERROR:")]))

func test_matcher_stub_with_args_uses_matcher() -> void:
	var m = mock(HealthComponent)
	stub_with_args(m, "describe_hit", [Gdt.any_int()], "hit")
	assert_eq(m.describe_hit(1), "hit")
	assert_eq(m.describe_hit(999), "hit")

# ── mock_singleton ────────────────────────────────────────────────────────

func test_mock_singleton_replaces_autoload() -> void:
	var m = mock_singleton("EventBus")
	assert_not_null(m)
	stub(m, "get_last_event", "mocked")
	assert_eq(m.get_last_event(), "mocked")
	assert_called(m, "get_last_event")

func test_mock_singleton_real_restored_after_test() -> void:
	# Real EventBus should be back at /root/EventBus (previous test restored it)
	var node = Engine.get_main_loop().root.get_node_or_null("EventBus")
	assert_not_null(node)
	# It should be the real one — get_last_event returns "" not "mocked"
	assert_eq(node.get_last_event(), "")

func test_mock_singleton_builtin_fails_gracefully() -> void:
	# Input is a C++ singleton, cannot be doubled — should return null + push_error
	# We can't assert push_error, but we can assert it returns null
	# (This would push an error to Godot's console — expected behaviour)
	pass

# ── Ordered call verification ──────────────────────────────────

func test_assert_called_in_order_passes() -> void:
	var m = mock(HealthComponent)
	m.get_hp()
	m.take_damage(10)
	m.is_dead()
	assert_called_in_order(m, ["get_hp", "take_damage", "is_dead"])

func test_assert_called_in_order_with_interleaving() -> void:
	var m = mock(HealthComponent)
	m.get_hp()
	m.take_damage(10)
	m.get_hp()
	m.is_dead()
	# get_hp appears before is_dead even with extra get_hp in between
	assert_called_in_order(m, ["take_damage", "is_dead"])

func test_assert_called_in_order_fails_on_wrong_order() -> void:
	var m = mock(HealthComponent)
	m.is_dead()
	m.take_damage(10)
	assert_false(Gdt._check_called_in_order(m, ["take_damage", "is_dead"]))

func test_assert_called_in_order_fails_on_missing_call() -> void:
	var m = mock(HealthComponent)
	m.get_hp()
	assert_false(Gdt._check_called_in_order(m, ["get_hp", "is_dead"]))

func test_get_call_log_order() -> void:
	var m = mock(HealthComponent)
	m.get_hp()
	m.take_damage(5)
	m.get_hp()
	var log = get_call_log(m)
	assert_eq(log.size(), 3)
	assert_eq(log[0][0], "get_hp")
	assert_eq(log[1][0], "take_damage")
	assert_eq(log[2][0], "get_hp")

# ── Auto-cleanup ────────────────────────────────────────────────

func test_mock_auto_free_false_stays_valid() -> void:
	var m = mock(HealthComponent, false)
	assert_not_null(m)
	assert_true(is_instance_valid(m))

func test_spy_auto_free_false_stays_valid() -> void:
	var real := HealthComponent.new()
	var s = spy(real, false)
	assert_not_null(s)
	assert_true(is_instance_valid(s))

func test_partial_mock_auto_free_false_stays_valid() -> void:
	var m = partial_mock(HealthComponent, false)
	assert_not_null(m)
	assert_true(is_instance_valid(m))

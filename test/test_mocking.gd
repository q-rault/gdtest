extends GdTest

const HealthComponent = preload("support/health_component.gd")

func test_mock_records_call() -> void:
	var m = mock(HealthComponent)
	m.get_hp()
	assert_called(m, "get_hp")

func test_stub_return_value() -> void:
	var m = mock(HealthComponent)
	stub(m, "get_hp", 42)
	assert_eq(m.get_hp(), 42)

func test_stub_default_returns_null() -> void:
	var m = mock(HealthComponent)
	assert_null(m.get_hp())

func test_stub_with_args_specific_call() -> void:
	var m = mock(HealthComponent)
	stub_with_args(m, "take_damage", [100], "heavy hit")
	assert_eq(m.take_damage(100), "heavy hit")

func test_stub_with_args_falls_back_to_generic() -> void:
	var m = mock(HealthComponent)
	stub(m, "take_damage", "default")
	stub_with_args(m, "take_damage", [100], "heavy")
	assert_eq(m.take_damage(1), "default")
	assert_eq(m.take_damage(100), "heavy")

func test_assert_not_called() -> void:
	var m = mock(HealthComponent)
	assert_not_called(m, "take_damage")

func test_assert_call_count_single() -> void:
	var m = mock(HealthComponent)
	m.get_hp()
	assert_call_count(m, "get_hp", 1)

func test_assert_call_count_multiple() -> void:
	var m = mock(HealthComponent)
	m.get_hp()
	m.get_hp()
	m.get_hp()
	assert_call_count(m, "get_hp", 3)

func test_assert_called_with_args() -> void:
	var m = mock(HealthComponent)
	m.take_damage(50)
	assert_called_with(m, "take_damage", [50])

func test_assert_called_with_multiple_calls() -> void:
	var m = mock(HealthComponent)
	m.take_damage(10)
	m.take_damage(25)
	assert_called_with(m, "take_damage", [10])
	assert_called_with(m, "take_damage", [25])

func test_get_calls_empty_before_any_call() -> void:
	var m = mock(HealthComponent)
	var calls = get_calls(m, "get_hp")
	assert_eq(calls.size(), 0)

func test_get_calls_returns_all_invocations() -> void:
	var m = mock(HealthComponent)
	m.take_damage(10)
	m.take_damage(20)
	var calls = get_calls(m, "take_damage")
	assert_eq(calls.size(), 2)
	assert_eq(calls[0], [10])
	assert_eq(calls[1], [20])

func test_mock_isolates_calls_per_instance() -> void:
	var m1 = mock(HealthComponent)
	var m2 = mock(HealthComponent)
	m1.get_hp()
	assert_called(m1, "get_hp")
	assert_not_called(m2, "get_hp")

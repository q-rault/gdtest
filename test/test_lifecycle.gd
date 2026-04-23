extends GdTest

var _counter: int = 0
var _setup_calls: int = 0
var _teardown_calls: int = 0

func before_each() -> void:
	_counter = 10
	_setup_calls += 1

func after_each() -> void:
	_teardown_calls += 1

func test_before_each_sets_counter() -> void:
	assert_eq(_counter, 10)

func test_before_each_runs_fresh_each_time() -> void:
	# If before_each resets _counter to 10, this always sees 10 regardless of order.
	assert_eq(_counter, 10)

func test_mutation_does_not_persist() -> void:
	_counter = 999
	assert_eq(_counter, 999)
	# next test should see 10 again from before_each

func test_counter_reset_after_mutation() -> void:
	assert_eq(_counter, 10)

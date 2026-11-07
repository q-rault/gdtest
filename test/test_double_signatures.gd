extends GdTest

# Generated doubles must compile for every method shape a script can declare,
# and must honour each method's declared return type.

const Fixture = preload("support/double_fixture.gd")


func test_mock_compiles_for_every_signature() -> void:
	assert_not_null(mock(Fixture))


func test_spy_compiles_for_every_signature() -> void:
	assert_not_null(spy(Fixture.new()))


func test_partial_mock_compiles_for_every_signature() -> void:
	assert_not_null(partial_mock(Fixture))


func test_mock_is_a_subclass_of_the_doubled_script() -> void:
	assert_true(mock(Fixture) is Fixture)


func test_unstubbed_typed_methods_return_zero_values() -> void:
	var m = mock(Fixture)
	assert_eq(m.overridden(), 0)
	assert_eq(m.inherited(), 0)
	assert_eq(m.flag(), false)
	assert_eq(m.ratio(), 0.0)
	assert_eq(m.kind(), 0)
	assert_eq(m.name_of(), &"")
	assert_eq(m.position(), Vector2.ZERO)
	assert_eq(m.raw(), PackedByteArray())
	assert_eq(m.plain_array(), [])
	assert_eq(m.defaults(1), "")


func test_unstubbed_typed_collections_keep_their_element_type() -> void:
	var m = mock(Fixture)
	var numbers: Array[int] = m.numbers()
	var nodes: Array[Node] = m.nodes()
	var table: Dictionary[String, int] = m.table()
	assert_eq(numbers.size(), 0)
	assert_eq(nodes.size(), 0)
	assert_eq(table.size(), 0)


func test_unstubbed_object_and_variant_methods_return_null() -> void:
	var m = mock(Fixture)
	assert_null(m.owner_node())
	assert_null(m.any_value())
	assert_null(m.untyped(1))


func test_stubs_apply_to_typed_methods() -> void:
	var m = mock(Fixture)
	var two_numbers: Array[int] = [1, 2]
	stub(m, "overridden", 42)
	stub(m, "any_value", "text")
	stub(m, "numbers", two_numbers)
	assert_eq(m.overridden(), 42)
	assert_eq(m.any_value(), "text")
	assert_eq(m.numbers().size(), 2)


func test_default_arguments_are_recorded() -> void:
	var m = mock(Fixture)
	m.defaults(1)
	assert_called_with(m, "defaults", [1, 3, "s", Vector2.ONE, []])


func test_void_methods_record_calls() -> void:
	var m = mock(Fixture)
	m.note("hello")
	assert_called_with(m, "note", ["hello"])


func test_static_methods_are_left_alone() -> void:
	assert_eq(Fixture.make(), 7)


func test_partial_mock_runs_real_code_with_default_arguments() -> void:
	var m = partial_mock(Fixture)
	assert_eq(m.defaults(1), "1 3 s (1.0, 1.0) 0")
	assert_eq(m.overridden(), 2)
	assert_eq(m.inherited(), 1)
	assert_called(m, "defaults")


func test_partial_mock_stub_overrides_real_code() -> void:
	var m = partial_mock(Fixture)
	stub(m, "overridden", 9)
	assert_eq(m.overridden(), 9)


func test_spy_runs_void_methods_and_records_them() -> void:
	var s = spy(Fixture.new())
	s.note("hello")
	assert_called(s, "note")

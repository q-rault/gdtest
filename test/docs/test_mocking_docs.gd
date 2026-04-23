## Living docs for mocking.mdx — regions embedded verbatim in the page.
extends GdTest

# region: mock_action
func test_battle_uses_ai_action() -> void:
	var ai := mock(Enemy)
	stub(ai, "choose_action", "attack")
	var result := Battle.resolve({"attack": 10}, {"defense": 3}, ai)
	assert_eq(result["action"], "attack")
	assert_eq(result["damage"], 7)
	assert_called(ai, "choose_action")
# endregion

# region: call_count
func test_ai_consulted_once_per_resolve() -> void:
	var ai := mock(Enemy)
	stub(ai, "choose_action", "patrol")
	Battle.resolve({"attack": 5}, {"defense": 2}, ai)
	assert_call_count(ai, "choose_action", 1)
# endregion

# region: patrol_no_damage
func test_patrol_does_no_damage() -> void:
	var ai := mock(Enemy)
	stub(ai, "choose_action", "patrol")
	var result := Battle.resolve({"attack": 10}, {"defense": 3}, ai)
	assert_eq(result["damage"], 0)
	assert_not_called(ai, "in_range")
# endregion

## Living docs for assertions.mdx — every region here is embedded verbatim in the page.
extends GdTest

# region: eq
func test_eq() -> void:
	assert_eq(Battle.calc_damage(10, 3), 7)
	assert_ne(Battle.calc_damage(10, 3), 0)
	assert_gt(Battle.calc_damage(10, 0), 0)
	assert_lte(Battle.calc_damage(1, 99), 0)
# endregion

# region: almost_eq
func test_almost_eq() -> void:
	assert_almost_eq(1.0 / 3.0, 0.333333, 1e-4)
# endregion

# region: bool_null
func test_bool_null() -> void:
	var h := Health.new(100)
	assert_true(h.hp > 0)
	h.take_damage(100)
	assert_false(h.hp > 0)
	assert_null(null)
	assert_not_null(h)
# endregion

# region: is
func test_is() -> void:
	assert_is(Enemy.new(), Enemy)
	assert_is(Spawner.new().spawn_wave(1)[0], Enemy)
# endregion

# region: between_has
func test_between_has() -> void:
	assert_between(Battle.calc_damage(10, 3), 0, 20)
	var loot := ["sword", "shield", "potion"]
	assert_has(loot, "sword")
	assert_not_has(loot, "staff")
# endregion

# region: string
func test_string() -> void:
	var data := SaveGame.serialise({"name": "Aria", "hp": 80, "xp": 0, "inventory": []})
	assert_string_contains(data["name"], "Aria")
	assert_string_starts_with(data["name"], "Ar")
	assert_string_ends_with(data["name"], "ia")
# endregion

func get_test_damage_cases() -> Array:
	return [
		["zero defense", [10,  0, 10]],
		["some defense", [10,  3,  7]],
		["no damage",    [1,  99,  0]],
	]

# region: parameterized
func test_damage(attack: int, defense: int, expected: int) -> void:
	assert_eq(Battle.calc_damage(attack, defense), expected)
# endregion

# region: describe
class describe_Battle extends GdTest:
	func test_basic_hit() -> void:
		assert_eq(Battle.calc_damage(10, 3), 7)

	func test_crit() -> void:
		assert_eq(Battle.calc_damage(10, 3, true), 14)
# endregion

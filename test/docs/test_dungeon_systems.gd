## Smoke tests for the shared Dungeon Fighter systems used in docs examples.
## If these fail, docs snippets that reference these classes are broken.
extends GdTest

# region: battle_basic_hit
func test_battle_basic_hit() -> void:
	assert_eq(Battle.calc_damage(10, 3), 7)
# endregion

# region: battle_crit
func test_battle_crit_doubles_damage() -> void:
	assert_eq(Battle.calc_damage(10, 3, true), 14)
# endregion

# region: battle_no_negative
func test_battle_no_negative_damage() -> void:
	assert_eq(Battle.calc_damage(1, 99), 0)
# endregion

# region: health_take_damage
func test_health_take_damage() -> void:
	var h := Health.new(100)
	h.take_damage(30)
	assert_eq(h.hp, 70)
# endregion

# region: health_died_signal
func test_health_emits_died() -> void:
	var h := Health.new(10)
	var fired: Array[int] = [0]
	h.died.connect(func() -> void: fired[0] += 1)
	h.take_damage(10)
	assert_eq(fired[0], 1)
# endregion

# region: health_heal_clamped
func test_health_heal_clamped_to_max() -> void:
	var h := Health.new(100)
	h.take_damage(40)
	h.heal(200)
	assert_eq(h.hp, 100)
# endregion

# region: enemy_patrol
func test_enemy_starts_patrolling() -> void:
	var e := Enemy.new()
	assert_eq(e.choose_action(), "patrol")
# endregion

# region: enemy_state_machine
func test_enemy_state_machine() -> void:
	var e := Enemy.new()
	e.alert()
	assert_eq(e.choose_action(), "chase")
	e.in_range()
	assert_eq(e.choose_action(), "attack")
# endregion

# region: inventory_bonus
func test_inventory_stat_bonus() -> void:
	var inv := Inventory.new()
	inv.add_item({"attack": 5})
	inv.add_item({"attack": 3, "defense": 2})
	assert_eq(inv.stat_bonus("attack"), 8)
	assert_eq(inv.stat_bonus("defense"), 2)
# endregion

# region: save_game_roundtrip
func test_save_game_roundtrip() -> void:
	var hero := {"name": "Aria", "hp": 80, "xp": 120, "inventory": []}
	var data := SaveGame.serialise(hero)
	var loaded := SaveGame.deserialise(data)
	assert_eq(loaded["name"], "Aria")
	assert_eq(loaded["hp"], 80)
	assert_eq(loaded["xp"], 120)
# endregion

# region: spawner_wave
func test_spawner_wave_count() -> void:
	var s := Spawner.new()
	var wave := s.spawn_wave(3)
	assert_eq(wave.size(), 3)
	assert_is(wave[0], Enemy)
# endregion

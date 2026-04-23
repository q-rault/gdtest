## Living docs for snapshots.mdx — regions embedded verbatim in the page.
extends GdTest

# region: save_snapshot
func test_hero_save_snapshot() -> void:
	var hero := {"name": "Aria", "hp": 80, "xp": 120, "inventory": ["sword", "potion"]}
	assert_snapshot(SaveGame.serialise(hero), "hero_save")
# endregion

# region: named_snapshots
func test_save_roundtrip_snapshots() -> void:
	var before := {"name": "Aria", "hp": 100, "xp": 0, "inventory": []}
	var after  := {"name": "Aria", "hp": 80,  "xp": 120, "inventory": ["sword"]}
	assert_snapshot(SaveGame.serialise(before), "before")
	assert_snapshot(SaveGame.serialise(after),  "after")
# endregion

class_name SaveGame extends RefCounted

static func serialise(hero: Dictionary) -> Dictionary:
	return {
		"name":      hero.get("name", ""),
		"hp":        hero.get("hp", 0),
		"xp":        hero.get("xp", 0),
		"inventory": hero.get("inventory", []),
	}

static func deserialise(data: Dictionary) -> Dictionary:
	return {
		"name":      data.get("name", ""),
		"hp":        data.get("hp", 0),
		"xp":        data.get("xp", 0),
		"inventory": data.get("inventory", []),
	}

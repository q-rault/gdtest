class_name Battle extends RefCounted

static func calc_damage(attack: int, defense: int, crit: bool = false) -> int:
	var base: int = maxi(0, attack - defense)
	return base * 2 if crit else base

static func resolve(attacker: Dictionary, defender: Dictionary, ai: Object) -> Dictionary:
	var action: String = ai.choose_action()
	var dmg: int = 0
	if action == "attack":
		dmg = calc_damage(attacker.get("attack", 0), defender.get("defense", 0))
	return {"action": action, "damage": dmg}

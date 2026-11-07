extends RefCounted

var _hp: int = 100

func get_hp() -> int:
	return _hp

func take_damage(amount: int) -> void:
	_hp -= amount

func heal(amount: int) -> void:
	_hp += amount

func is_dead() -> bool:
	return _hp <= 0

func describe_hit(amount: int) -> String:
	return "hit for %d" % amount

func last_note():
	return null

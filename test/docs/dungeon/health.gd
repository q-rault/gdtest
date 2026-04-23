class_name Health extends RefCounted

signal died

var hp: int
var max_hp: int

func _init(p_max_hp: int) -> void:
	max_hp = p_max_hp
	hp = p_max_hp

func take_damage(amount: int) -> void:
	hp = maxi(0, hp - amount)
	if hp == 0:
		died.emit()

func heal(amount: int) -> void:
	hp = mini(max_hp, hp + amount)

class_name HealthPotion extends Node

signal picked_up(amount: int)

@export var heal_amount: int = 20

func collect(h: Health) -> void:
	h.heal(heal_amount)
	picked_up.emit(heal_amount)

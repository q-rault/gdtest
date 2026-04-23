extends RefCounted

signal pinged
signal hit(damage: int)
signal moved(x: float, y: float)

func ping() -> void:
	emit_signal("pinged")

func take_hit(damage: int) -> void:
	emit_signal("hit", damage)

func move(x: float, y: float) -> void:
	emit_signal("moved", x, y)

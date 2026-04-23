extends Resource

@export var speed: float = 50.0
@export var damage: int = 10
@export_range(0.0, 100.0, 1.0) var health_pct: float = 100.0
var not_exported: int = 0

var _max_hp: int = 200
var max_hp: int:
	get: return _max_hp
	set(_v): pass

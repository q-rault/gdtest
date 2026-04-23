class_name Enemy extends RefCounted

enum State { PATROL, CHASE, ATTACK }

var state: State = State.PATROL
var health: Health

func _init() -> void:
	health = Health.new(100)

func choose_action() -> String:
	match state:
		State.ATTACK: return "attack"
		State.CHASE:  return "chase"
		_:            return "patrol"

func alert() -> void:
	state = State.CHASE

func in_range() -> void:
	state = State.ATTACK

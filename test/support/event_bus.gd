extends Node

var _last_event: String = ""

func emit_event(event_name: String) -> void:
	_last_event = event_name

func get_last_event() -> String:
	return _last_event

func clear() -> void:
	_last_event = ""

extends "double_parent.gd"

enum Kind { A, B }

var with_setter: int = 0:
	set(value):
		with_setter = value

func overridden() -> int:
	return 2

func defaults(a: int, b: int = 3, c := "s", d: Vector2 = Vector2.ONE, e: Array = []) -> String:
	return "%d %d %s %s %d" % [a, b, c, d, e.size()]

static func make() -> int:
	return 7

func numbers() -> Array[int]:
	return [1]

func nodes() -> Array[Node]:
	return []

func table() -> Dictionary[String, int]:
	return {"a": 1}

func plain_array() -> Array:
	return [1]

func kind() -> Kind:
	return Kind.B

func untyped(x):
	return x

func any_value() -> Variant:
	return 1

func name_of() -> StringName:
	return &"real"

func position() -> Vector2:
	return Vector2.ONE

func raw() -> PackedByteArray:
	return PackedByteArray([1])

func callback() -> Callable:
	return Callable()

func owner_node() -> Node:
	return null

func flag() -> bool:
	return true

func ratio() -> float:
	return 1.5

func note(_text: String) -> void:
	pass

class_name Inventory extends RefCounted

var _items: Array[Dictionary] = []

func add_item(item: Dictionary) -> void:
	_items.append(item)

func stat_bonus(stat: String) -> int:
	var total := 0
	for item: Dictionary in _items:
		total += item.get(stat, 0)
	return total

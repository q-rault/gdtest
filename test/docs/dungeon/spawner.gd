class_name Spawner extends RefCounted

func spawn_wave(count: int) -> Array:
	var result: Array = []
	for i: int in count:
		result.append(Enemy.new())
	return result

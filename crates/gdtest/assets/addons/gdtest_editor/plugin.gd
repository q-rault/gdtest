@tool
extends EditorPlugin

var _panel: Control

func _enter_tree() -> void:
	_panel = preload("res://addons/gdtest_editor/test_panel.gd").new()
	_panel.name = "gdtest"
	_panel.editor_interface = get_editor_interface()
	add_control_to_bottom_panel(_panel, "gdtest")

	get_editor_interface().get_resource_filesystem().filesystem_changed.connect(
		_panel._on_filesystem_changed
	)

func _exit_tree() -> void:
	if not is_instance_valid(_panel):
		return
	var fs := get_editor_interface().get_resource_filesystem()
	if fs.filesystem_changed.is_connected(_panel._on_filesystem_changed):
		fs.filesystem_changed.disconnect(_panel._on_filesystem_changed)
	remove_control_from_bottom_panel(_panel)
	_panel.queue_free()
	_panel = null

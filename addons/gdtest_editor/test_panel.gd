@tool
extends Control

const GDTEST_OUT_PREFIX := "GDTEST_OUT:"
const _SPINNER_CHARS := ["⠋","⠙","⠹","⠸","⠼","⠴","⠦","⠧","⠇","⠏"]

var editor_interface: EditorInterface

var _tree: Tree
var _run_all_btn: Button
var _run_file_btn: Button
var _rerun_failed_btn: Button
var _kill_btn: Button
var _watch_btn: CheckButton
var _status_label: Label
var _gdtest_path: String = "gdtest"
var _failed_tests: Array[Dictionary] = []
var _watch_timer: Timer
var _is_running: bool = false
var _poll_timer: Timer = null
var _gdtest_pid: int = -1
var _gdtest_out_file: String = ""
var _run_start_time: int = 0
const _MAX_RUN_SECONDS := 120

var _spinner_timer: Timer = null
var _spinner_frame: int = 0

var _run_file_items: Dictionary = {}
var _run_file_pass: Dictionary = {}
var _run_file_fail: Dictionary = {}
var _run_root: TreeItem = null
var _gdtest_read_offset: int = 0

func _ready() -> void:
	_build_ui()
	_find_gdtest()
	_populate_test_tree_from_filesystem()

func _build_ui() -> void:
	var vbox := VBoxContainer.new()
	add_child(vbox)
	vbox.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)

	var toolbar := HBoxContainer.new()
	toolbar.add_theme_constant_override("separation", 4)
	vbox.add_child(toolbar)

	_run_all_btn = Button.new()
	_run_all_btn.text = "▶ Run All"
	_run_all_btn.tooltip_text = "Run all tests"
	_run_all_btn.pressed.connect(_on_run_all_pressed)
	toolbar.add_child(_run_all_btn)

	_run_file_btn = Button.new()
	_run_file_btn.text = "▶ Run File"
	_run_file_btn.tooltip_text = "Run tests in the selected file"
	_run_file_btn.disabled = true
	_run_file_btn.pressed.connect(_on_run_file_pressed)
	toolbar.add_child(_run_file_btn)

	_rerun_failed_btn = Button.new()
	_rerun_failed_btn.text = "↺ Re-run Failed"
	_rerun_failed_btn.tooltip_text = "Re-run only tests that failed last time"
	_rerun_failed_btn.disabled = true
	_rerun_failed_btn.pressed.connect(_on_rerun_failed_pressed)
	toolbar.add_child(_rerun_failed_btn)

	_kill_btn = Button.new()
	_kill_btn.text = "■ Kill"
	_kill_btn.tooltip_text = "Kill the running gdtest process"
	_kill_btn.disabled = true
	_kill_btn.pressed.connect(_on_kill_pressed)
	toolbar.add_child(_kill_btn)

	toolbar.add_child(VSeparator.new())

	_watch_btn = CheckButton.new()
	_watch_btn.text = "Watch"
	_watch_btn.tooltip_text = "Re-run all tests automatically when a file is saved"
	toolbar.add_child(_watch_btn)

	_status_label = Label.new()
	_status_label.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	_status_label.horizontal_alignment = HORIZONTAL_ALIGNMENT_RIGHT
	toolbar.add_child(_status_label)

	_tree = Tree.new()
	_tree.size_flags_vertical = Control.SIZE_EXPAND_FILL
	_tree.columns = 2
	_tree.set_column_title(0, "Test")
	_tree.set_column_title(1, "Time")
	_tree.set_column_expand(0, true)
	_tree.set_column_expand(1, false)
	_tree.set_column_custom_minimum_width(1, 80)
	_tree.column_titles_visible = true
	_tree.allow_rmb_select = true
	_tree.item_activated.connect(_on_item_activated)
	_tree.item_mouse_selected.connect(_on_item_mouse_selected)
	_tree.item_selected.connect(_on_item_selected)
	vbox.add_child(_tree)

	_watch_timer = Timer.new()
	_watch_timer.wait_time = 1.5
	_watch_timer.one_shot = true
	_watch_timer.timeout.connect(_on_run_all_pressed)
	add_child(_watch_timer)

	_spinner_timer = Timer.new()
	_spinner_timer.wait_time = 0.12
	_spinner_timer.timeout.connect(_tick_spinner)
	add_child(_spinner_timer)

func _find_gdtest() -> void:
	var project_root := ProjectSettings.globalize_path("res://")
	# 1. GDTEST_BIN environment variable
	var env_bin := OS.get_environment("GDTEST_BIN")
	if env_bin != "" and FileAccess.file_exists(env_bin):
		_gdtest_path = env_bin
		return
	# 2. Search common relative locations (project dir, parent, sibling gdtest project)
	for rel: String in [
			"gdtest.exe", "gdtest",
			"../gdtest.exe", "../gdtest",
			"target/debug/gdtest.exe", "target/debug/gdtest",
			"../gdtest/target/debug/gdtest.exe", "../gdtest/target/debug/gdtest",
			]:
		var candidate := project_root.path_join(rel)
		if FileAccess.file_exists(candidate):
			_gdtest_path = candidate
			return
	# 3. Fall back to PATH
	_gdtest_path = "gdtest"

func _populate_test_tree_from_filesystem() -> void:
	_tree.clear()
	var root := _tree.create_item()
	root.set_text(0, "Tests (not yet run)")
	_scan_test_dir("res://test", root)

func _scan_test_dir(dir_path: String, parent: TreeItem) -> void:
	var dir := DirAccess.open(dir_path)
	if dir == null:
		return
	dir.list_dir_begin()
	var fname := dir.get_next()
	while fname != "":
		if dir.current_is_dir() and not fname.begins_with("."):
			_scan_test_dir(dir_path.path_join(fname), parent)
		elif fname.begins_with("test_") and fname.ends_with(".gd"):
			var item := _tree.create_item(parent)
			item.set_text(0, fname)
			item.set_text(1, "—")
			item.set_metadata(0, {type = "file", file = dir_path.path_join(fname)})
		fname = dir.get_next()

func _collect_test_files(dir_path: String) -> Array[String]:
	var result: Array[String] = []
	var dir := DirAccess.open(dir_path)
	if dir == null:
		return result
	dir.list_dir_begin()
	var fname := dir.get_next()
	while fname != "":
		if dir.current_is_dir() and not fname.begins_with("."):
			result.append_array(_collect_test_files(dir_path.path_join(fname)))
		elif fname.begins_with("test_") and fname.ends_with(".gd"):
			result.append(dir_path.path_join(fname))
		fname = dir.get_next()
	return result

func _rebuild_tree_for_run(res_paths: Array[String]) -> void:
	_tree.clear()
	_run_file_items.clear()
	_run_file_pass.clear()
	_run_file_fail.clear()
	_run_root = _tree.create_item()
	_run_root.set_text(0, "Running…")
	var files: Array[String] = res_paths if not res_paths.is_empty() else _collect_test_files("res://test")
	for f: String in files:
		var item := _tree.create_item(_run_root)
		item.set_text(0, _SPINNER_CHARS[0] + "  " + f.get_file())
		item.set_text(1, "")
		item.collapsed = true
		item.set_metadata(0, {type = "file", file = f})
		_run_file_items[f] = item
		_run_file_pass[f] = 0
		_run_file_fail[f] = 0

# --- Signal handlers ---

func _on_item_selected() -> void:
	var item := _tree.get_selected()
	_run_file_btn.disabled = _is_running or item == null

func _on_item_activated() -> void:
	var item := _tree.get_selected()
	if item == null:
		return
	var meta: Variant = item.get_metadata(0)
	if meta is Dictionary:
		_open_in_editor(meta.get("file", ""), meta.get("line", 0), meta.get("name", ""))

func _on_item_mouse_selected(position: Vector2, mouse_button_index: int) -> void:
	if mouse_button_index != MOUSE_BUTTON_RIGHT:
		return
	var item := _tree.get_selected()
	if item == null:
		return
	var meta: Variant = item.get_metadata(0)
	if not meta is Dictionary:
		return

	var menu := PopupMenu.new()
	add_child(menu)
	menu.popup_hide.connect(menu.queue_free)

	var item_type: String = meta.get("type", "")
	match item_type:
		"file":
			menu.add_item("▶ Run File", 0)
			menu.add_item("Open in Editor", 1)
		"test":
			menu.add_item("▶ Run This Test", 0)
			menu.add_item("Open in Editor", 1)
		_:
			menu.queue_free()
			return

	menu.id_pressed.connect(func(id: int) -> void:
		match id:
			0:
				if item_type == "file":
					_run_specific_file(meta.get("file", ""))
				else:
					_run_specific_test(meta.get("file", ""), meta.get("name", ""))
			1:
				_open_in_editor(meta.get("file", ""), meta.get("line", 0), meta.get("name", ""))
	)
	menu.position = DisplayServer.mouse_get_position()
	menu.popup()

func _on_run_all_pressed() -> void:
	if _is_running:
		return
	_run([], "")

func _on_run_file_pressed() -> void:
	if _is_running:
		return
	var item := _tree.get_selected()
	if item == null:
		return
	var meta: Variant = item.get_metadata(0)
	if not meta is Dictionary:
		return
	_run_specific_file(meta.get("file", ""))

func _on_rerun_failed_pressed() -> void:
	if _is_running or _failed_tests.is_empty():
		return
	var seen: Dictionary = {}
	var files: Array[String] = []
	for t: Dictionary in _failed_tests:
		var f: String = t.get("file", "")
		if f != "" and not seen.has(f):
			seen[f] = true
			files.append(f)
	_run(files, "")

# Called by plugin.gd via EditorFileSystem.filesystem_changed
func _on_filesystem_changed() -> void:
	if not _watch_btn.button_pressed or _is_running:
		return
	_watch_timer.start()

# --- Run helpers ---

func _run_specific_file(res_path: String) -> void:
	_run([res_path], "")

func _run_specific_test(res_path: String, test_name: String) -> void:
	_run([res_path], test_name)

func _run(res_paths: Array[String], filter: String) -> void:
	_set_running(true)
	_gdtest_read_offset = 0
	_rebuild_tree_for_run(res_paths)
	_spinner_timer.start()
	_status_label.remove_theme_color_override("font_color")

	var project_root := ProjectSettings.globalize_path("res://")
	var args: Array[String] = [
		"--godot-exe", OS.get_executable_path(),
		"--project-path", project_root,
		"--no-auto-import",
		"--no-rewrite",
		"--reporter", "json",
	]
	if res_paths.is_empty():
		args.append(ProjectSettings.globalize_path("res://test/"))
	else:
		for p: String in res_paths:
			args.append(ProjectSettings.globalize_path(p) if p.begins_with("res://") else p)
	if filter != "":
		args.append_array(["--filter", "^%s$" % filter])

	_gdtest_out_file = OS.get_temp_dir().path_join(
		"gdtest_out_%d.json" % Time.get_ticks_msec()
	)
	# cd to project root first so the relative harness path resolves correctly.
	var project_root_win := project_root.replace("/", "\\")
	var out_file_win := _gdtest_out_file.replace("/", "\\")
	var parts: PackedStringArray = PackedStringArray()
	parts.append('"%s"' % _gdtest_path.replace("/", "\\"))
	for a in args:
		parts.append('"%s"' % a.replace('"', '\\"'))
	var cmd := 'cd /D "%s" && %s > "%s" 2>&1' % [
		project_root_win.rstrip("\\"),
		" ".join(parts),
		out_file_win,
	]
	print("[gdtest] cmd: ", cmd)

	_run_start_time = Time.get_ticks_msec()
	_gdtest_pid = OS.create_process("cmd.exe", ["/C", cmd])
	print("[gdtest] pid=", _gdtest_pid)

	if _poll_timer == null:
		_poll_timer = Timer.new()
		_poll_timer.wait_time = 2.0
		_poll_timer.timeout.connect(_poll_gdtest)
		add_child(_poll_timer)
	_poll_timer.start()

func _poll_gdtest() -> void:
	_parse_incremental()
	var elapsed_s := (Time.get_ticks_msec() - _run_start_time) / 1000
	var still_running := _gdtest_pid >= 0 and OS.is_process_running(_gdtest_pid)

	# Show partial output size so we know gdtest is alive and producing output.
	var f_diag := FileAccess.open(_gdtest_out_file, FileAccess.READ)
	var cur_bytes := f_diag.get_length() if f_diag != null else 0
	if f_diag != null:
		f_diag.close()
	print("[gdtest] poll %ds pid=%d running=%s output=%d bytes" % [elapsed_s, _gdtest_pid, still_running, cur_bytes])

	if still_running and elapsed_s >= _MAX_RUN_SECONDS:
		print("[gdtest] timeout after %ds — killing pid=%d" % [_MAX_RUN_SECONDS, _gdtest_pid])
		OS.kill(_gdtest_pid)
		_poll_timer.stop()
		_gdtest_pid = -1
		_status_label.text = "Timed out (%ds)" % _MAX_RUN_SECONDS
		_status_label.add_theme_color_override("font_color", Color(0.9, 0.3, 0.3))
		_set_running(false)
		return

	if not still_running:
		_poll_timer.stop()
		_gdtest_pid = -1
		var f := FileAccess.open(_gdtest_out_file, FileAccess.READ)
		var raw := f.get_as_text() if f != null else ""
		if f != null:
			f.close()
		print("[gdtest] done, raw bytes=", raw.length())
		_parse_and_display(raw)
		_set_running(false)

func _on_kill_pressed() -> void:
	if _gdtest_pid >= 0:
		print("[gdtest] killing pid=", _gdtest_pid)
		OS.kill(_gdtest_pid)
		_poll_timer.stop()
		_gdtest_pid = -1
		_status_label.text = "Killed"
		_set_running(false)

func _set_running(running: bool) -> void:
	_is_running = running
	_run_all_btn.disabled = running
	_run_file_btn.disabled = running or _tree.get_selected() == null
	_rerun_failed_btn.disabled = running or _failed_tests.is_empty()
	_kill_btn.disabled = not running

func _tick_spinner() -> void:
	_spinner_frame = (_spinner_frame + 1) % _SPINNER_CHARS.size()
	var ch: String = _SPINNER_CHARS[_spinner_frame]
	_status_label.text = ch + "  Running…"
	for f: String in _run_file_items:
		var item: TreeItem = _run_file_items[f]
		var txt := item.get_text(0)
		if txt.length() > 0:
			item.set_text(0, ch + txt.substr(1))

func _parse_incremental() -> void:
	if _gdtest_out_file == "":
		return
	var f := FileAccess.open(_gdtest_out_file, FileAccess.READ)
	if f == null:
		return
	var total_bytes := f.get_length()
	if total_bytes <= _gdtest_read_offset:
		f.close()
		return
	f.seek(_gdtest_read_offset)
	var new_bytes := f.get_buffer(total_bytes - _gdtest_read_offset)
	_gdtest_read_offset = total_bytes
	f.close()

	var new_text := new_bytes.get_string_from_utf8()
	for raw_line: String in new_text.split("\n"):
		var trimmed := raw_line.strip_edges()
		if not trimmed.begins_with("{"):
			continue
		var data: Variant = JSON.parse_string(trimmed)
		if not data is Dictionary:
			continue
		var kind: String = data.get("kind", "")
		if kind == "summary" or kind == "stderr_noise" or kind == "worker_crashed":
			continue
		var name: String = data.get("name", "")
		var file: String = data.get("file", "")
		if file == "" or name == "":
			continue

		var file_item: TreeItem = _run_file_items.get(file)
		if file_item == null:
			if _run_root == null:
				continue
			file_item = _tree.create_item(_run_root)
			file_item.collapsed = true
			file_item.set_metadata(0, {type = "file", file = file})
			_run_file_items[file] = file_item
			_run_file_pass[file] = 0
			_run_file_fail[file] = 0

		match kind:
			"test_pass":
				_run_file_pass[file] += 1
				var ti := _tree.create_item(file_item)
				ti.set_text(0, "✓  " + name)
				ti.set_text(1, _fmt_us(data.get("us", 0)))
				ti.set_custom_color(0, Color(0.4, 0.9, 0.4))
				ti.set_metadata(0, {type = "test", file = file, name = name, line = 0})
			"test_fail":
				_run_file_fail[file] += 1
				var ti := _tree.create_item(file_item)
				ti.set_text(0, "✗  " + name)
				ti.set_text(1, _fmt_us(data.get("us", 0)))
				ti.set_custom_color(0, Color(0.9, 0.3, 0.3))
				ti.set_metadata(0, {type = "test", file = file, name = name, line = 0})
				file_item.collapsed = false
			"test_error":
				_run_file_fail[file] += 1
				var ti := _tree.create_item(file_item)
				ti.set_text(0, "!  " + name)
				ti.set_text(1, "—")
				ti.set_custom_color(0, Color(1.0, 0.6, 0.0))
				ti.set_metadata(0, {type = "test", file = file, name = name, line = 0})
				file_item.collapsed = false
			"test_skip", "test_pending":
				var ti := _tree.create_item(file_item)
				ti.set_text(0, "⊘  " + name)
				ti.set_text(1, "—")
				ti.set_custom_color(0, Color(0.7, 0.7, 0.7))
				ti.set_metadata(0, {type = "test", file = file, name = name, line = 0})

		var p: int = _run_file_pass.get(file, 0)
		var ff: int = _run_file_fail.get(file, 0)
		var ch: String = _SPINNER_CHARS[_spinner_frame]
		var summary := "  (%d✓" % p
		if ff > 0:
			summary += "  %d✗" % ff
		summary += ")"
		file_item.set_text(0, ch + "  " + file.get_file() + summary)

# --- Output parsing ---

func _parse_and_display(raw: String) -> void:
	_spinner_timer.stop()
	_run_file_items.clear()
	_run_file_pass.clear()
	_run_file_fail.clear()
	_tree.clear()
	_failed_tests.clear()
	var root := _tree.create_item()
	root.set_text(0, "Test Results")

	# Collect all result events first
	var events: Array = []
	for raw_line: String in raw.split("\n"):
		var trimmed := raw_line.strip_edges()
		if not trimmed.begins_with("{"):
			continue
		var data: Variant = JSON.parse_string(trimmed)
		if not data is Dictionary:
			continue
		var kind: String = data.get("kind", "")
		if kind == "summary" or kind == "stderr_noise" or kind == "worker_crashed":
			continue
		events.append(data)

	# Build per-file declaration-order index so we can sort by source position
	var file_decl_order: Dictionary = {}  # file → {func_name → line_index}
	for ev: Variant in events:
		var file: String = (ev as Dictionary).get("file", "")
		if file == "" or file_decl_order.has(file):
			continue
		var order: Dictionary = {}
		var script := load(file) as Script
		if script != null:
			var src_lines := script.source_code.split("\n")
			for i in src_lines.size():
				var stripped: String = src_lines[i].strip_edges()
				if stripped.begins_with("func ") and "(" in stripped:
					var fname: String = stripped.substr(5, stripped.find("(") - 5).strip_edges()
					if not order.has(fname):
						order[fname] = i
		file_decl_order[file] = order

	# Sort by file path then by declaration line within each file
	events.sort_custom(func(a: Variant, b: Variant) -> bool:
		var fa: String = (a as Dictionary).get("file", "")
		var fb: String = (b as Dictionary).get("file", "")
		if fa != fb:
			return fa < fb
		var la: int = (file_decl_order.get(fa, {}) as Dictionary).get((a as Dictionary).get("name", ""), 999999)
		var lb: int = (file_decl_order.get(fb, {}) as Dictionary).get((b as Dictionary).get("name", ""), 999999)
		return la < lb
	)

	var pass_count := 0
	var fail_count := 0
	var error_count := 0
	var skip_count := 0
	var file_items: Dictionary = {}  # res_path → TreeItem

	for ev: Variant in events:
		var data := ev as Dictionary
		var kind: String = data.get("kind", "")
		var name: String = data.get("name", "")
		var file: String = data.get("file", "")
		var file_item: TreeItem = _get_or_create_file_item(root, file, file_items)

		match kind:
			"test_pass":
				pass_count += 1
				var item := _tree.create_item(file_item)
				item.set_text(0, "✓  " + name)
				item.set_text(1, _fmt_us(data.get("us", 0)))
				item.set_custom_color(0, Color(0.4, 0.9, 0.4))
				item.set_metadata(0, {type = "test", file = file, name = name, line = 0})

			"test_fail":
				fail_count += 1
				var item := _tree.create_item(file_item)
				item.set_text(0, "✗  " + name)
				item.set_text(1, _fmt_us(data.get("us", 0)))
				item.set_custom_color(0, Color(0.9, 0.3, 0.3))
				var first_line := 0
				for f: Variant in data.get("failures", []):
					var fi := _tree.create_item(item)
					fi.set_text(0, "   → " + str(f))
					fi.set_custom_color(0, Color(0.9, 0.5, 0.3))
					var ln := _parse_line_number(str(f))
					if ln > 0 and first_line == 0:
						first_line = ln
					fi.set_metadata(0, {type = "test", file = file, name = name, line = ln})
				item.set_metadata(0, {type = "test", file = file, name = name, line = first_line})
				_failed_tests.append({file = file, name = name})

			"test_error":
				error_count += 1
				var item := _tree.create_item(file_item)
				item.set_text(0, "!  " + name)
				item.set_text(1, "—")
				item.set_custom_color(0, Color(1.0, 0.6, 0.0))
				item.set_metadata(0, {type = "test", file = file, name = name, line = 0})
				var msg: String = data.get("message", "")
				if msg != "":
					var fi := _tree.create_item(item)
					fi.set_text(0, "   " + msg)
					fi.set_metadata(0, {type = "test", file = file, name = name, line = 0})
				_failed_tests.append({file = file, name = name})

			"test_skip", "test_pending":
				skip_count += 1
				var item := _tree.create_item(file_item)
				item.set_text(0, "⊘  " + name)
				item.set_text(1, "—")
				item.set_custom_color(0, Color(0.7, 0.7, 0.7))
				item.set_metadata(0, {type = "test", file = file, name = name, line = 0})

	# Finalize per-file summary text and collapse state
	for f: String in file_items:
		var fi: TreeItem = file_items[f]
		var fp := 0
		var ff := 0
		var child := fi.get_first_child()
		while child != null:
			var ctxt := child.get_text(0)
			if ctxt.begins_with("✓"):
				fp += 1
			elif ctxt.begins_with("✗") or ctxt.begins_with("!"):
				ff += 1
			child = child.get_next()
		var total_f := fp + ff
		var fprefix := "✓  " if ff == 0 else "✗  "
		fi.set_text(0, fprefix + f.get_file() + "  %d/%d" % [fp, total_f])
		fi.set_custom_color(0, Color(0.4, 0.9, 0.4) if ff == 0 else Color(0.9, 0.3, 0.3))
		fi.collapsed = (ff == 0)

	var total := pass_count + fail_count + error_count
	var parts: PackedStringArray = ["%d/%d passed" % [pass_count, total]]
	if skip_count > 0:
		parts.append("%d skipped" % skip_count)
	_status_label.text = "  ".join(parts)
	if fail_count > 0 or error_count > 0:
		_status_label.add_theme_color_override("font_color", Color(0.9, 0.3, 0.3))
	else:
		_status_label.add_theme_color_override("font_color", Color(0.4, 0.9, 0.4))
	_rerun_failed_btn.disabled = _failed_tests.is_empty()

# --- Utilities ---

func _open_in_editor(res_path: String, line: int, func_name: String = "") -> void:
	if res_path == "" or editor_interface == null:
		return
	var script := load(res_path) as Script
	if script == null:
		return
	editor_interface.set_main_screen_editor("Script")
	var target_line := line
	if func_name != "":
		var found := _find_func_line(script, func_name)
		if found > 0:
			target_line = found
	editor_interface.edit_script(script, target_line if target_line > 0 else -1, 0)
	if target_line > 0:
		_goto_line_deferred.call_deferred(target_line)

func _find_func_line(script: Script, func_name: String) -> int:
	var lines := script.source_code.split("\n")
	for i in lines.size():
		if lines[i].strip_edges().begins_with("func " + func_name + "("):
			return i + 1
	return 0

func _goto_line_deferred(target_line: int) -> void:
	var se := editor_interface.get_script_editor()
	if se == null:
		return
	var cur := se.get_current_editor()
	if cur == null:
		return
	var code_edit := cur.get_base_editor()
	if code_edit == null:
		return
	code_edit.set_caret_line(target_line - 1, false)
	var max_scroll := maxi(0, code_edit.get_line_count() - code_edit.get_visible_line_count())
	code_edit.set_v_scroll(mini(target_line - 1, max_scroll))

func _get_or_create_file_item(root: TreeItem, file: String, file_items: Dictionary) -> TreeItem:
	if file_items.has(file):
		return file_items[file]
	var item := _tree.create_item(root)
	item.set_text(0, file.get_file() if file != "" else "unknown")
	item.set_text(1, "")
	item.collapsed = true
	item.set_metadata(0, {type = "file", file = file})
	file_items[file] = item
	return item

func _parse_line_number(msg: String) -> int:
	var regex := RegEx.new()
	regex.compile(r":(\d+)")
	var result := regex.search(msg)
	if result:
		return result.get_string(1).to_int()
	return 0

static func _fmt_us(us: Variant) -> String:
	var n := int(us)
	if n < 1000:
		return "%d µs" % n
	elif n < 1000000:
		return "%.2f ms" % (n / 1000.0)
	return "%.2f s" % (n / 1000000.0)

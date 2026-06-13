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
var _gdtest_out_dir: String = ""
var _run_start_time: int = 0
const _MAX_RUN_SECONDS := 120

var _spinner_timer: Timer = null
var _spinner_frame: int = 0

var _icon_idle: ImageTexture
var _icon_running: ImageTexture
var _icon_pass: ImageTexture
var _icon_fail: ImageTexture
var _icon_error: ImageTexture
var _icon_skip: ImageTexture

var _run_file_items: Dictionary = {}
var _run_file_pass: Dictionary = {}
var _run_file_fail: Dictionary = {}
var _run_root: TreeItem = null
var _gdtest_read_seq: int = 0
var _run_test_items: Dictionary = {}
var _file_base_labels: Dictionary = {}
var _test_base_labels: Dictionary = {}
var _run_test_status: Dictionary = {}
var _run_file_total: Dictionary = {}
var _run_file_done: Dictionary = {}
var _run_file_active: Dictionary = {}
var _run_file_time_us: Dictionary = {}
var _run_target_files: Dictionary = {}
var _run_target_tests: Dictionary = {}

func _ready() -> void:
	_build_ui()
	_build_icons()
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
	_tree.set_column_custom_minimum_width(1, 115)
	_tree.column_titles_visible = true
	_tree.hide_root = true
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

func _build_icons() -> void:
	_icon_idle    = _make_dot_icon(Color(0.55, 0.55, 0.55, 0.35))
	_icon_running = _make_dot_icon(Color(0.90, 0.78, 0.10))
	_icon_pass    = _make_dot_icon(Color(0.35, 0.88, 0.35))
	_icon_fail    = _make_dot_icon(Color(0.90, 0.25, 0.25))
	_icon_error   = _make_dot_icon(Color(1.00, 0.55, 0.10))
	_icon_skip    = _make_dot_icon(Color(0.55, 0.55, 0.55, 0.60))

static func _make_dot_icon(color: Color, size: int = 12) -> ImageTexture:
	var img := Image.create(size, size, false, Image.FORMAT_RGBA8)
	var cx := size * 0.5
	var cy := size * 0.5
	var r2 := (size * 0.32) * (size * 0.32)
	for y in size:
		for x in size:
			var dx := (x + 0.5) - cx
			var dy := (y + 0.5) - cy
			if dx * dx + dy * dy <= r2:
				img.set_pixel(x, y, color)
	return ImageTexture.create_from_image(img)

func _find_gdtest() -> void:
	var project_root := ProjectSettings.globalize_path("res://")
	# 1. GDTEST_BIN environment variable
	var env_bin := OS.get_environment("GDTEST_BIN")
	if env_bin != "" and FileAccess.file_exists(env_bin):
		_gdtest_path = env_bin
		return
	# 2. Pick the newest common local binary so a stale debug build does not
	# override a freshly rebuilt release binary (or vice versa).
	var candidates: Array[String] = []
	for rel: String in [
			"target/release/gdtest.exe", "target/release/gdtest",
			"target/debug/gdtest.exe", "target/debug/gdtest",
			"../gdtest/target/release/gdtest.exe", "../gdtest/target/release/gdtest",
			"../gdtest/target/debug/gdtest.exe", "../gdtest/target/debug/gdtest",
			"gdtest.exe", "gdtest",
			"../gdtest.exe", "../gdtest",
			]:
		var candidate := project_root.path_join(rel)
		if FileAccess.file_exists(candidate):
			candidates.append(candidate)
	if not candidates.is_empty():
		_gdtest_path = _newest_file_path(candidates)
		return
	# 3. Fall back to PATH
	_gdtest_path = "gdtest"

func _newest_file_path(paths: Array[String]) -> String:
	var best_path := paths[0]
	var best_mtime := FileAccess.get_modified_time(best_path)
	for p: String in paths:
		var mtime := FileAccess.get_modified_time(p)
		if mtime > best_mtime:
			best_mtime = mtime
			best_path = p
	return best_path

func _populate_test_tree_from_filesystem() -> void:
	_tree.clear()
	_run_file_items.clear()
	_run_test_items.clear()
	_file_base_labels.clear()
	_test_base_labels.clear()
	_run_root = null
	var root := _tree.create_item()
	_run_root = root
	_populate_test_tree_from_gdtest(root)

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

func _populate_test_tree_from_gdtest(root: TreeItem) -> bool:
	var output: Array = []
	var project_root := ProjectSettings.globalize_path("res://")
	var args: Array[String] = [
		"--project-path", project_root,
		"--no-auto-import",
		"--list",
		"--reporter", "json",
		ProjectSettings.globalize_path("res://test/"),
	]
	var exit_code := OS.execute(_gdtest_path, args, output, true, false)
	if exit_code != 0:
		_add_discovery_message(
			root,
			_discovery_error_message(exit_code, output),
			Color(0.9, 0.3, 0.3)
		)
		return false

	var file_items: Dictionary = {}
	var saw_any := false
	var saw_non_json_output := false
	for chunk: Variant in output:
		for raw_line: String in str(chunk).split("\n"):
			var trimmed := raw_line.strip_edges()
			if trimmed == "":
				continue
			if not trimmed.begins_with("{"):
				saw_non_json_output = true
				continue
			var data: Variant = JSON.parse_string(trimmed)
			if not data is Dictionary:
				saw_non_json_output = true
				continue
			if data.get("kind", "") != "test":
				continue
			var file: String = data.get("file", "")
			var name: String = data.get("name", "")
			if file == "" or name == "":
				continue
			saw_any = true

			var file_item: TreeItem = file_items.get(file)
			if file_item == null:
				file_item = _tree.create_item(root)
				file_item.set_icon(0, _icon_idle)
				file_item.set_text(0, file.get_file())
				file_item.set_text(1, "-")
				file_item.set_text_alignment(1, HORIZONTAL_ALIGNMENT_RIGHT)
				file_item.collapsed = true
				file_item.set_metadata(0, {type = "file", file = file})
				file_items[file] = file_item
				_run_file_items[file] = file_item
				_file_base_labels[file] = file.get_file()

			var test_item := _tree.create_item(file_item)
			var display_name := _display_name_for_test(name)
			test_item.set_icon(0, _icon_idle)
			test_item.set_text(0, display_name)
			test_item.set_text(1, "-")
			test_item.set_text_alignment(1, HORIZONTAL_ALIGNMENT_RIGHT)
			test_item.set_metadata(0, {
				type = "test",
				file = file,
				name = name,
				line = 0,
			})
			var test_key := _test_key(file, name)
			_run_test_items[test_key] = test_item
			_test_base_labels[test_key] = display_name
			_run_test_status[test_key] = "idle"
	if not saw_any:
		if saw_non_json_output:
			_add_discovery_message(
				root,
				"gdtest binary does not support JSON discovery output; rebuild/update the selected gdtest binary",
				Color(0.9, 0.3, 0.3)
			)
		else:
			_add_discovery_message(root, "No test files found", Color(0.7, 0.7, 0.7))
	return saw_any

func _display_name_for_test(name: String) -> String:
	if "/" not in name:
		return name
	var describe_name := name.get_slice("/", 0)
	var test_name := name.get_slice("/", 1)
	return describe_name.trim_prefix("describe_") + " / " + test_name

func _test_key(file: String, name: String) -> String:
	return file + "::" + name

func _add_discovery_message(root: TreeItem, message: String, color: Color) -> void:
	var item := _tree.create_item(root)
	item.set_text(0, message)
	item.set_text(1, "")
	item.set_custom_color(0, color)
	item.selectable = false

func _discovery_error_message(exit_code: int, output: Array) -> String:
	var details := ""
	for chunk: Variant in output:
		for raw_line: String in str(chunk).split("\n"):
			var trimmed := raw_line.strip_edges()
			if trimmed == "" or trimmed.begins_with("{"):
				continue
			details = trimmed
			break
		if details != "":
			break
	if details != "":
		return "gdtest discovery failed: %s" % details
	if _gdtest_path == "gdtest":
		return "gdtest is not configured or not available on PATH"
	return "gdtest discovery failed (exit code %d)" % exit_code

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

func _prepare_tree_for_run(res_paths: Array[String], filter: String = "", preserve_existing: bool = false) -> void:
	if _run_root == null or _run_file_items.is_empty():
		_populate_test_tree_from_filesystem()
	if _run_root == null:
		return
	_run_file_pass.clear()
	_run_file_fail.clear()
	_run_file_total.clear()
	_run_file_done.clear()
	_run_file_active.clear()
	_run_file_time_us.clear()
	_run_target_files.clear()
	_run_target_tests.clear()
	_failed_tests.clear()

	var selected_files: Dictionary = {}
	var explicit_tests: Dictionary = {}
	if res_paths.is_empty():
		for f: String in _run_file_items:
			selected_files[f] = true
	else:
		for spec: String in res_paths:
			if "::" in spec:
				var file := spec.get_slice("::", 0)
				var name := spec.get_slice("::", 1)
				selected_files[file] = true
				explicit_tests[_test_key(file, name)] = true
			else:
				selected_files[spec] = true

	for file: String in _run_file_items:
		_run_file_pass[file] = 0
		_run_file_fail[file] = 0
		_run_file_total[file] = 0
		_run_file_done[file] = 0
		_run_file_active[file] = 0
		var file_item: TreeItem = _run_file_items[file]
		if not preserve_existing:
			file_item.set_icon(0, _icon_idle)
			file_item.set_text(0, _file_base_labels.get(file, file.get_file()))
			file_item.set_text(1, "")
			file_item.clear_custom_color(0)

	for test_key: String in _run_test_items:
		var test_item: TreeItem = _run_test_items[test_key]
		var meta: Variant = test_item.get_metadata(0)
		var file := ""
		var name := ""
		if meta is Dictionary:
			file = meta.get("file", "")
			name = meta.get("name", "")
		var should_run := selected_files.has(file)
		if not explicit_tests.is_empty():
			should_run = explicit_tests.has(test_key)
		if filter != "":
			should_run = should_run and name == filter

		if preserve_existing and not should_run:
			var current_status := str(_run_test_status.get(test_key, "idle"))
			if current_status == "pass":
				_run_file_pass[file] = int(_run_file_pass.get(file, 0)) + 1
				_run_file_done[file] = int(_run_file_done.get(file, 0)) + 1
				_run_file_total[file] = int(_run_file_total.get(file, 0)) + 1
			elif current_status == "fail" or current_status == "error":
				_run_file_fail[file] = int(_run_file_fail.get(file, 0)) + 1
				_run_file_done[file] = int(_run_file_done.get(file, 0)) + 1
				_run_file_total[file] = int(_run_file_total.get(file, 0)) + 1
			elif current_status == "skip":
				_run_file_done[file] = int(_run_file_done.get(file, 0)) + 1
				_run_file_total[file] = int(_run_file_total.get(file, 0)) + 1
			continue

		if should_run:
			_run_target_tests[test_key] = true
			_run_target_files[file] = true
			_run_file_total[file] = int(_run_file_total.get(file, 0)) + 1
		_run_test_status[test_key] = "idle"
		test_item.set_icon(0, _icon_idle)
		test_item.set_text(0, _test_base_labels.get(test_key, name))
		test_item.set_text(1, "-" if should_run else "")
		test_item.clear_custom_color(0)

	for file: String in _run_target_files:
		var fi: TreeItem = _run_file_items.get(file)
		if fi != null:
			_update_file_item(file)

func _ensure_file_item(file: String) -> TreeItem:
	var item: TreeItem = _run_file_items.get(file)
	if item != null:
		return item
	if _run_root == null:
		return null
	item = _tree.create_item(_run_root)
	item.set_icon(0, _icon_idle)
	item.set_text(0, file.get_file())
	item.set_text(1, "")
	item.set_text_alignment(1, HORIZONTAL_ALIGNMENT_RIGHT)
	item.collapsed = false
	item.set_metadata(0, {type = "file", file = file})
	_run_file_items[file] = item
	_file_base_labels[file] = file.get_file()
	_run_file_pass[file] = 0
	_run_file_fail[file] = 0
	_run_file_total[file] = 0
	_run_file_done[file] = 0
	_run_file_active[file] = 0
	_run_file_time_us[file] = 0
	return item

func _ensure_test_item(file: String, name: String) -> TreeItem:
	var key := _test_key(file, name)
	var item: TreeItem = _run_test_items.get(key)
	if item != null:
		return item
	var file_item := _ensure_file_item(file)
	if file_item == null:
		return null
	item = _tree.create_item(file_item)
	var display_name := _display_name_for_test(name)
	item.set_icon(0, _icon_idle)
	item.set_text(0, display_name)
	item.set_text(1, "-")
	item.set_text_alignment(1, HORIZONTAL_ALIGNMENT_RIGHT)
	item.set_metadata(0, {type = "test", file = file, name = name, line = 0})
	_run_test_items[key] = item
	_test_base_labels[key] = display_name
	_run_test_status[key] = "idle"
	return item

func _update_file_item(file: String) -> void:
	var item: TreeItem = _run_file_items.get(file)
	if item == null:
		return
	var base := str(_file_base_labels.get(file, file.get_file()))
	var pass_count := int(_run_file_pass.get(file, 0))
	var fail_count := int(_run_file_fail.get(file, 0))
	var total := int(_run_file_total.get(file, 0))
	var done := int(_run_file_done.get(file, 0))
	var active := int(_run_file_active.get(file, 0))
	var file_time_us := int(_run_file_time_us.get(file, 0))
	var summary := ""
	if total > 0:
		summary = "  %d/%d" % [pass_count, total]
		if fail_count > 0:
			summary += "  %d❌" % fail_count
	if active > 0:
		item.set_icon(0, _icon_running)
		item.set_text(0, base + summary)
		item.set_text(1, "")
		item.clear_custom_color(0)
	elif total > 0 and done >= total:
		item.set_icon(0, _icon_pass if fail_count == 0 else _icon_fail)
		item.set_text(0, base + summary)
		item.set_text(1, _fmt_us(file_time_us))
		item.set_custom_color(0, Color(0.4, 0.9, 0.4) if fail_count == 0 else Color(0.9, 0.3, 0.3))
	else:
		item.set_icon(0, _icon_idle)
		item.set_text(0, base + summary)
		item.set_text(1, "")
		item.clear_custom_color(0)

func _set_test_running(file: String, name: String) -> void:
	var item := _ensure_test_item(file, name)
	if item == null:
		return
	var key := _test_key(file, name)
	if not _run_target_tests.has(key):
		_run_target_tests[key] = true
		_run_target_files[file] = true
		_run_file_total[file] = int(_run_file_total.get(file, 0)) + 1
	_run_test_status[key] = "running"
	_run_file_active[file] = int(_run_file_active.get(file, 0)) + 1
	item.set_icon(0, _icon_running)
	item.set_text(0, str(_test_base_labels.get(key, name)))
	item.set_text(1, "")
	item.clear_custom_color(0)
	_update_file_item(file)

func _set_test_result(file: String, name: String, kind: String, us: Variant = 0) -> void:
	var item := _ensure_test_item(file, name)
	if item == null:
		return
	var key := _test_key(file, name)
	if not _run_target_tests.has(key):
		_run_target_tests[key] = true
		_run_target_files[file] = true
		_run_file_total[file] = int(_run_file_total.get(file, 0)) + 1
	var prev := str(_run_test_status.get(key, "idle"))
	if prev == "running":
		_run_file_active[file] = maxi(0, int(_run_file_active.get(file, 0)) - 1)
	if prev == "idle" or prev == "running":
		_run_file_done[file] = int(_run_file_done.get(file, 0)) + 1
	if kind == "test_pass":
		_run_file_pass[file] = int(_run_file_pass.get(file, 0)) + 1
		_run_file_time_us[file] = int(_run_file_time_us.get(file, 0)) + int(us)
		item.set_icon(0, _icon_pass)
		item.set_text(0, str(_test_base_labels.get(key, name)))
		item.set_text(1, _fmt_us(us))
		item.set_custom_color(0, Color(0.4, 0.9, 0.4))
		_run_test_status[key] = "pass"
	elif kind == "test_fail":
		_run_file_fail[file] = int(_run_file_fail.get(file, 0)) + 1
		_run_file_time_us[file] = int(_run_file_time_us.get(file, 0)) + int(us)
		item.set_icon(0, _icon_fail)
		item.set_text(0, str(_test_base_labels.get(key, name)))
		item.set_text(1, _fmt_us(us))
		item.set_custom_color(0, Color(0.9, 0.3, 0.3))
		_run_test_status[key] = "fail"
		_failed_tests.append({file = file, name = name})
	elif kind == "test_error":
		_run_file_fail[file] = int(_run_file_fail.get(file, 0)) + 1
		item.set_icon(0, _icon_error)
		item.set_text(0, str(_test_base_labels.get(key, name)))
		item.set_text(1, "-")
		item.set_custom_color(0, Color(1.0, 0.6, 0.0))
		_run_test_status[key] = "error"
		_failed_tests.append({file = file, name = name})
	else:
		item.set_icon(0, _icon_skip)
		item.set_text(0, str(_test_base_labels.get(key, name)))
		item.set_text(1, "-")
		item.set_custom_color(0, Color(0.7, 0.7, 0.7))
		_run_test_status[key] = "skip"
	_update_file_item(file)

func _refresh_run_status_label() -> void:
	var pass_count := 0
	var fail_count := 0
	var skip_count := 0
	var running_count := 0
	var total_expected := _run_target_tests.size()

	for key: String in _run_target_tests:
		var status := str(_run_test_status.get(key, "idle"))
		match status:
			"pass":
				pass_count += 1
			"fail", "error":
				fail_count += 1
			"skip":
				skip_count += 1
			"running":
				running_count += 1

	var done_count := pass_count + fail_count + skip_count

	if _is_running:
		var in_progress := done_count + running_count
		var ch: String = _SPINNER_CHARS[_spinner_frame]
		_status_label.text = "%s  %d / %d tests" % [ch, in_progress, total_expected]
		_status_label.remove_theme_color_override("font_color")
	elif total_expected == 0:
		_status_label.text = ""
		_status_label.remove_theme_color_override("font_color")
	else:
		var total_us := 0
		for t: Variant in _run_file_time_us.values():
			total_us += int(t)
		var time_str := "  " + _fmt_us(total_us)
		if fail_count == 0:
			_status_label.text = "✓  %d%s" % [total_expected, time_str]
			_status_label.add_theme_color_override("font_color", Color(0.4, 0.9, 0.4))
		else:
			var pass_displayed := total_expected - fail_count
			if pass_displayed > 0:
				_status_label.text = "✗  %d   ✓  %d%s" % [fail_count, pass_displayed, time_str]
			else:
				_status_label.text = "✗  %d%s" % [fail_count, time_str]
			_status_label.add_theme_color_override("font_color", Color(0.9, 0.3, 0.3))

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
	var targets: Array[String] = []
	for t: Dictionary in _failed_tests:
		var f: String = t.get("file", "")
		var name: String = t.get("name", "")
		var key := _test_key(f, name)
		if f != "" and name != "" and not seen.has(key):
			seen[key] = true
			targets.append(f + "::" + name)
	_run(targets, "")

# Called by plugin.gd via EditorFileSystem.filesystem_changed
func _on_filesystem_changed() -> void:
	if _is_running:
		return
	_populate_test_tree_from_filesystem()
	if not _watch_btn.button_pressed:
		return
	_watch_timer.start()

# --- Run helpers ---

func _run_specific_file(res_path: String) -> void:
	_run([res_path], "")

func _run_specific_test(res_path: String, test_name: String) -> void:
	_run([res_path + "::" + test_name], "")

func _run(res_paths: Array[String], filter: String, preserve_existing: bool = false) -> void:
	_set_running(true)
	_cleanup_out_dir()
	_gdtest_read_seq = 0
	_prepare_tree_for_run(res_paths, filter, preserve_existing)
	_refresh_run_status_label()
	_spinner_timer.start()

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
			if "::" in p:
				var file_part := p.get_slice("::", 0)
				var test_part := p.get_slice("::", 1)
				var abs_file := ProjectSettings.globalize_path(file_part) if file_part.begins_with("res://") else file_part
				args.append(abs_file + "::" + test_part)
			else:
				args.append(ProjectSettings.globalize_path(p) if p.begins_with("res://") else p)
	if filter != "":
		args.append_array(["--filter", "^%s$" % filter])

	_gdtest_out_dir = OS.get_temp_dir().path_join(
		"gdtest_out_%d" % Time.get_ticks_msec()
	)
	args.append_array(["--output-dir", _gdtest_out_dir])

	_run_start_time = Time.get_ticks_msec()
	_gdtest_pid = OS.create_process(_gdtest_path, args)

	if _poll_timer == null:
		_poll_timer = Timer.new()
		_poll_timer.wait_time = 0.15
		_poll_timer.timeout.connect(_poll_gdtest)
		add_child(_poll_timer)
	_poll_timer.start()

func _poll_gdtest() -> void:
	_parse_incremental()
	var elapsed_s := (Time.get_ticks_msec() - _run_start_time) / 1000
	var still_running := _gdtest_pid >= 0 and OS.is_process_running(_gdtest_pid)

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
		_parse_incremental()
		_set_running(false)
		_parse_and_display()

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
	_refresh_run_status_label()

func _parse_incremental() -> void:
	if _gdtest_out_dir == "":
		return
	while true:
		var next_seq := _gdtest_read_seq + 1
		var fpath := _gdtest_out_dir.path_join("%08d.json" % next_seq)
		if not FileAccess.file_exists(fpath):
			break
		var f := FileAccess.open(fpath, FileAccess.READ)
		if f == null:
			# Rust is still writing this file; seq stays unchanged, retry next poll.
			break
		var content := f.get_as_text()
		f.close()
		_gdtest_read_seq = next_seq
		_handle_json_event(content)


func _handle_json_event(raw: String) -> void:
	var trimmed := raw.strip_edges()
	if not trimmed.begins_with("{"):
		return
	var data: Variant = JSON.parse_string(trimmed)
	if not data is Dictionary:
		return
	var kind: String = data.get("kind", "")
	if kind == "summary" or kind == "stderr_noise" or kind == "worker_crashed":
		return
	var name: String = data.get("name", "")
	var file: String = data.get("file", "")
	if file == "" or name == "":
		return
	match kind:
		"test_start":
			_set_test_running(file, name)
			_refresh_run_status_label()
		"test_pass", "test_fail", "test_error", "test_skip", "test_pending":
			_set_test_result(file, name, kind, data.get("us", 0))
			_refresh_run_status_label()


func _cleanup_out_dir() -> void:
	if _gdtest_out_dir == "" or not DirAccess.dir_exists_absolute(_gdtest_out_dir):
		_gdtest_out_dir = ""
		return
	var dir := DirAccess.open(_gdtest_out_dir)
	if dir != null:
		dir.list_dir_begin()
		var fname := dir.get_next()
		while fname != "":
			if not dir.current_is_dir():
				dir.remove(fname)
			fname = dir.get_next()
		dir.list_dir_end()
	DirAccess.remove_absolute(_gdtest_out_dir)
	_gdtest_out_dir = ""

# --- Output parsing ---

func _parse_and_display() -> void:
	_spinner_timer.stop()
	_refresh_run_status_label()
	_rerun_failed_btn.disabled = _failed_tests.is_empty()
	for file: String in _run_target_files:
		_update_file_item(file)

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
	if "/" in func_name:
		func_name = func_name.get_slice("/", 1)
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


static func _fmt_us(us: Variant) -> String:
	var n := int(us)
	if n < 10:
		return "<0.01 ms"
	var use_decimals := n < 1000  # < 1ms keeps 2 decimal places
	var ms_str := ("%.2f" % (n / 1000.0)) if use_decimals else ("%d" % (n / 1000))
	var dot := ms_str.find(".")
	var int_part := ms_str.left(dot) if dot >= 0 else ms_str
	var dec_part := ms_str.substr(dot) if dot >= 0 else ""
	var grouped := ""
	if int_part.length() > 3:
		var offset := int_part.length() % 3
		if offset > 0:
			grouped = int_part.left(offset)
		for i in range(offset, int_part.length(), 3):
			if grouped != "":
				grouped += " "
			grouped += int_part.substr(i, 3)
	else:
		grouped = int_part
	return grouped + dec_part + " ms"

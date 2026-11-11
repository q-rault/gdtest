extends SceneTree

# gdtest worker.
#
# Architecture:
# - A background Thread reads JSON commands off stdin into a Mutex-protected
#   queue. Stdin is blocking so it can't live on the main thread — otherwise
#   the main loop can't tick and `await physics_frame` never fires.
# - `_process` drains the queue on the main thread, handles each command as
#   a coroutine so tests can `await get_tree().physics_frame`,
#   `await some_signal`, etc.
# - The harness exposes an assertion API via `Engine.set_meta("gdt_harness", self)`.
#   Tests call `Engine.get_meta("gdt_harness").expect(cond, msg)` for
#   assertions and `await h.simulate_physics_frames(n)` to advance frames.
#
# Protocol reference: crates/gdtest/src/protocol.rs.

const OUT_PREFIX := "GDTEST_OUT:"

# Records script errors, such as a null dereference, as Godot reports them. A
# script error aborts the running function without failing any assertion, so a
# test that hits one would otherwise look like it passed. Observing them in
# process works the same on every platform, unlike correlating stderr from the
# CLI. Godot may log from any thread, hence the mutex.
class ScriptErrorLogger extends Logger:
	var _mutex := Mutex.new()
	var _errors: Array[String] = []

	func _log_error(
		function: String,
		file: String,
		line: int,
		code: String,
		rationale: String,
		_editor_notify: bool,
		error_type: int,
		_script_backtrace: Array[ScriptBacktrace]
	) -> void:
		if error_type != ERROR_TYPE_SCRIPT:
			return
		var message := rationale if rationale != "" else code
		_mutex.lock()
		_errors.append("%s (%s:%d in %s)" % [message, file, line, function])
		_mutex.unlock()

	func take_errors() -> Array[String]:
		_mutex.lock()
		var errors := _errors
		_errors = []
		_mutex.unlock()
		return errors

var _script_error_logger := ScriptErrorLogger.new()
const HARNESS_VERSION := "0.2.6"

var _current_failures: Array[String] = []
var _pending_msg: String = ""
var _skip_msg: String = ""
var _snapshot_counter: int = 0
var _current_container: Node = null
var _singleton_swaps: Dictionary = {}
var _mock_registry: Array = []
var _watcher_registry: Array = []
var _watcher_map: Dictionary = {}
var _emission_counter: int = 0
var _original_physics_tps: int = -1
var _orphan_baseline: int = -1
var _memory_baseline: int = -1
var _autoref_registry: Array = []
var _captured_errors: Array = []
var _captured_warnings: Array = []
var _cmd_queue: Array = []
var _cmd_mutex: Mutex = Mutex.new()
var _stdin_thread: Thread = null
var _stop_requested: bool = false
var _processing: bool = false
var _world_counter: int = 0
# Plugin registries loaded from GDTEST_PLUGINS env var.
var _reporter_plugins: Array = []  # instances of Gdt.GdtReporter subclasses
var _fixture_plugins: Array = []   # instances of Gdt.GdtFixture subclasses
var _current_test_file: String = ""  # res:// path of the currently-running test file
# Shared suite instance for files with before()/before_all() hooks (RunTest path).
var _active_suite_script: String = ""
var _active_suite_instance: Object = null

# ── TestWorld / ActorProxy inner classes ────────────────────────────

class ActorProxyImpl extends RefCounted:
	var instance: Object

	func _init(node: Object) -> void:
		instance = node

	func at(pos: Vector2) -> Object:
		if instance is Node2D:
			(instance as Node2D).position = pos
		return self

	func at3d(pos: Vector3) -> Object:
		if instance is Node3D:
			(instance as Node3D).position = pos
		return self

class TestWorldImpl extends RefCounted:
	var _container: Node
	var _harness_ref: Object

	func _init(container: Node, harness: Object) -> void:
		_container = container
		_harness_ref = harness

	func spawn(klass) -> Object:
		var instance = klass.new()
		if not instance is Node:
			push_error("TestWorld.spawn: class must extend Node")
			return null
		_container.add_child(instance)
		return ActorProxyImpl.new(instance)

	func physics_frames(n: int) -> void:
		await _harness_ref.simulate_physics_frames(n)

	func process_frames(n: int) -> void:
		await _harness_ref.simulate_process_frames_n(n)

	func ready() -> void:
		await _harness_ref.simulate_process_frame()

func _init() -> void:
	OS.add_logger(_script_error_logger)
	Engine.set_meta("gdt_harness", self)
	load("res://addons/gdtest/gdunit4_chain.gd")
	load("res://addons/gdtest/gdunit4_compat.gd")
	_load_plugins()
	_stdin_thread = Thread.new()
	_stdin_thread.start(_stdin_reader_loop)
	var _vi := Engine.get_version_info()
	_send({
		"event": "ready",
		"harness_version": HARNESS_VERSION,
		"godot_version": "%d.%d.%d" % [_vi.major, _vi.minor, _vi.patch],
	})

func _finalize() -> void:
	# stdin thread exits on EOF; the Rust side closes its stdin during
	# shutdown so wait_to_finish() returns promptly.
	if _stdin_thread != null and _stdin_thread.is_started():
		_stdin_thread.wait_to_finish()

func _stdin_reader_loop() -> void:
	while not _stop_requested:
		var line: String = OS.read_string_from_stdin(4096)
		if line.is_empty():
			_cmd_mutex.lock()
			_cmd_queue.append("__eof")
			_cmd_mutex.unlock()
			return
		var trimmed: String = line.strip_edges()
		if trimmed.is_empty():
			continue
		_cmd_mutex.lock()
		_cmd_queue.append(trimmed)
		_cmd_mutex.unlock()

func _process(_delta: float) -> bool:
	if _processing:
		return false
	_cmd_mutex.lock()
	if _cmd_queue.is_empty():
		_cmd_mutex.unlock()
		return false
	# Dequeue exactly one command per _process tick. The handler may be a
	# coroutine; it awaits completion before returning and the outer
	# _processing flag keeps concurrent re-entry out. We fire-and-forget
	# here (no outer await) so _process itself stays cheap — the handler
	# clears _processing when it's done.
	var item: Variant = _cmd_queue.pop_front()
	_cmd_mutex.unlock()
	if item == "__eof":
		await _finish_active_suite()
		Gdt._cov_dump_file()
		_stop_requested = true
		quit(0)
		return true
	_processing = true
	_dispatch_command(item)
	return false

# Called fire-and-forget from _process. MUST be the last thing that can
# await in the chain — when it returns, _processing is released.
func _dispatch_command(line: String) -> void:
	await _handle_command(line)
	_processing = false

func _handle_command(line: String) -> void:
	var parsed: Variant = JSON.parse_string(line)
	if typeof(parsed) != TYPE_DICTIONARY:
		_send({"event": "log", "level": "error", "message": "invalid json: %s" % line})
		return
	var cmd: Dictionary = parsed
	var kind: String = cmd.get("cmd", "")
	match kind:
		"ping":
			_send({"event": "pong"})
		"run_test":
			await _run_test(cmd.get("script", ""), cmd.get("func", ""))
		"run_file":
			await _run_file(cmd.get("script", ""))
			_send({"event": "ready"})
		"shutdown":
			await _finish_active_suite()
			Gdt._cov_dump_file()
			_stop_requested = true
			quit(0)
		_:
			_send({"event": "log", "level": "warn", "message": "unknown cmd: %s" % kind})

# ── Assertion API (called by tests) ────────────────────────────────────────

# Marks the current test as pending (intentionally incomplete). Does not stop
# execution — call `return` after if needed. Overrides pass/fail result.
func pending(msg: String = "not yet implemented") -> void:
	_pending_msg = msg

# Marks the current test as skipped. Does not stop execution — call `return`
# after if needed. Overrides any pass/fail/pending result.
func skip(msg: String = "") -> void:
	_skip_msg = msg

# Appends a failure when cond is false. Returns cond so callers can chain.
func expect(cond: bool, msg: String = "") -> bool:
	if not cond:
		_current_failures.append(msg if msg != "" else "expectation failed")
	return cond

# Returns the number of failures recorded so far in the current test.
# Used by assert_all to detect whether a callable block added new failures.
func failure_count() -> int:
	return _current_failures.size()

# Trim the failure list back to `n` entries — used by forall() to discard
# failures accumulated during shrinking trial runs.
func trim_failures(n: int) -> void:
	while _current_failures.size() > n:
		_current_failures.pop_back()

# ── Snapshot assertions ────────────────────────────────────────────
#
# Snapshots live next to the test file:
#   test/test_player.gd  →  test/__snapshots__/test_player/<name>.gdsnap
# Set GDTEST_UPDATE_SNAPSHOTS=1 (or run with --update-snapshots) to accept
# new values. The first run always creates the file and passes.

func assert_snapshot(actual: Variant, snap_name: String, script_path: String) -> bool:
	var key: String = snap_name if snap_name != "" else str(_snapshot_counter)
	_snapshot_counter += 1
	var rel: String = script_path.trim_prefix("res://")
	var parts: PackedStringArray = rel.split("/")
	var file_stem: String = parts[-1].get_basename()
	parts.resize(parts.size() - 1)
	var base_dir: String = "/".join(parts)
	var snap_dir: String = "res://%s/__snapshots__/%s" % [base_dir, file_stem]
	var snap_path: String = "%s/%s.gdsnap" % [snap_dir, key]
	var actual_str: String = var_to_str(actual)
	var update: bool = OS.get_environment("GDTEST_UPDATE_SNAPSHOTS") != ""
	if not FileAccess.file_exists(snap_path) or update:
		DirAccess.make_dir_recursive_absolute(ProjectSettings.globalize_path(snap_dir))
		var f := FileAccess.open(snap_path, FileAccess.WRITE)
		if f == null:
			return expect(false, "snapshot: could not write %s (err %d)" % [snap_path, FileAccess.get_open_error()])
		f.store_string(actual_str)
		f.close()
		return true
	var f := FileAccess.open(snap_path, FileAccess.READ)
	if f == null:
		return expect(false, "snapshot: could not read %s (err %d)" % [snap_path, FileAccess.get_open_error()])
	var stored: String = f.get_as_text()
	f.close()
	if actual_str != stored:
		return expect(false, "snapshot mismatch '%s':\n  stored: %s\n  actual: %s\n  (run with --update-snapshots to accept)" % [key, stored, actual_str])
	return true

# Awaits `n` physics frames. Tests use this instead of
# `await get_tree().physics_frame` because Godot's physics signal fires
# during the harness's main loop — this helper avoids the boilerplate.
func simulate_physics_frames(n: int = 1) -> void:
	for i in n:
		await physics_frame

# Awaits enough physics frames to cover `seconds` at the project's
# physics tick rate. Rounds to the nearest tick; min 1 tick.
func simulate_physics_seconds(seconds: float) -> void:
	var ticks: int = int(round(seconds * float(Engine.physics_ticks_per_second)))
	await simulate_physics_frames(max(1, ticks))

# Awaits the next process frame. Rarely needed — prefer physics_frame for
# deterministic tests.
func simulate_process_frame() -> void:
	await process_frame

func simulate_process_frames_n(n: int) -> void:
	for i in n:
		await process_frame

func create_test_world() -> Object:
	var container := Node.new()
	container.name = "__gdt_world_%d" % _world_counter
	_world_counter += 1
	root.add_child(container)
	autoref(container)
	return TestWorldImpl.new(container, self)

# Polls `predicate` every process frame until it returns true or `timeout_ms`
# elapses. Records a test failure on timeout.
func wait_until(predicate: Callable, timeout_ms: float = 2000.0) -> void:
	var deadline: int = Time.get_ticks_msec() + int(timeout_ms)
	while not predicate.call():
		if Time.get_ticks_msec() >= deadline:
			expect(false, "wait_until timed out after %.0fms — condition never became true" % timeout_ms)
			return
		await process_frame

# Polls `predicate` every process frame until it returns false or `timeout_ms`
# elapses. Records a test failure on timeout.
func wait_while(predicate: Callable, timeout_ms: float = 2000.0) -> void:
	var deadline: int = Time.get_ticks_msec() + int(timeout_ms)
	while predicate.call():
		if Time.get_ticks_msec() >= deadline:
			expect(false, "wait_while timed out after %.0fms — condition never became false" % timeout_ms)
			return
		await process_frame

# ── Scene tree helpers ──────────────────────────────────────────

# Adds `node` as a child of the per-test container so it gets freed when
# the test finishes. Returns the node for chaining. The container is
# created lazily on first call so pure-RefCounted tests that never reach
# this method don't pay the per-test process_frame await.
func add_child_autofree(node: Node) -> Node:
	_ensure_container()
	_current_container.add_child(node)
	return node

func _ensure_container() -> void:
	if _current_container != null:
		return
	var c := Node.new()
	c.name = "_GdTest"
	root.add_child(c)
	_current_container = c

# Convenience wrapper: instantiate `packed` and parent the result under
# the per-test container. The scene's `_ready()` fires synchronously
# inside `add_child`; if the scene's children rely on autoloads or
# signals settling, the test should follow with
# `await Gdt.wait_process_frame()`.
func instantiate_scene(packed: PackedScene) -> Node:
	var n: Node = packed.instantiate()
	return add_child_autofree(n)

func _script_has_suite_hooks(gds: GDScript) -> bool:
	for m in gds.get_script_method_list():
		var n: String = str(m.get("name", ""))
		if n == "before_all" or n == "after_all" or n == "before" or n == "after":
			return true
	return false

func _finish_active_suite() -> void:
	if _active_suite_instance == null:
		_active_suite_script = ""
		return
	var inst: Object = _active_suite_instance
	_active_suite_instance = null
	_active_suite_script = ""
	if inst.has_method("after_all"):
		await inst.call("after_all")
	elif inst.has_method("after"):
		await inst.call("after")
	_free_instance(inst)

func _acquire_test_instance(gds: GDScript, script_path: String) -> Object:
	if not _script_has_suite_hooks(gds):
		await _finish_active_suite()
		return gds.new()
	if script_path != _active_suite_script:
		await _finish_active_suite()
		_active_suite_instance = gds.new()
		_active_suite_script = script_path
		if _active_suite_instance.has_method("before_all"):
			await _active_suite_instance.call("before_all")
		elif _active_suite_instance.has_method("before"):
			await _active_suite_instance.call("before")
	return _active_suite_instance

func _release_test_instance(instance: Object, instance_in_tree: bool) -> void:
	if instance == _active_suite_instance:
		return
	if not instance_in_tree:
		_free_instance(instance)

# ── Test dispatch ──────────────────────────────────────────────────────────

func _run_test(script_path: String, func_name: String) -> void:
	_current_failures.clear()
	_pending_msg = ""
	_skip_msg = ""
	_snapshot_counter = 0
	if script_path == "" or func_name == "":
		_send({"event": "test_error", "name": func_name, "message": "missing script or func"})
		return
	var resource: Resource = load(script_path)
	if resource == null:
		_send({"event": "test_error", "name": func_name, "message": "could not load script: %s" % script_path})
		return
	if not (resource is GDScript):
		_send({"event": "test_error", "name": func_name, "message": "not a GDScript: %s" % script_path})
		return
	var gds: GDScript = resource
	if not gds.can_instantiate():
		_send({"event": "test_error", "name": func_name, "message": "script cannot be instantiated (parse error?): %s" % script_path})
		return
	# describe_ block: `func_name` is "describe_Foo/test_bar" — instantiate the inner class.
	if "/" in func_name:
		var parts: PackedStringArray = func_name.split("/", false, 1)
		var inner_class_name: String = parts[0]
		var inner_func_name: String = parts[1]
		var inner_type: Variant = gds.get_script_constant_map().get(inner_class_name, null)
		if inner_type == null or not (inner_type is Script):
			_send({"event": "test_error", "name": func_name, "message": "inner class not found: %s in %s" % [inner_class_name, script_path]})
			return
		var inner_instance: Object = (inner_type as Script).new()
		if inner_instance == null:
			_send({"event": "test_error", "name": func_name, "message": "could not instantiate inner class %s" % inner_class_name})
			return
		await _run_describe_test(inner_instance, inner_func_name, func_name)
		return
	var instance: Object = await _acquire_test_instance(gds, script_path)
	if instance == null:
		_send({"event": "test_error", "name": func_name, "message": "could not instantiate %s" % script_path})
		return
	if not instance.has_method(func_name):
		_send({"event": "test_error", "name": func_name, "message": "no such func `%s` in %s" % [func_name, script_path]})
		_release_test_instance(instance, instance is Node)
		return
	# Parameterized expansion: if a companion `get_<func_name>_cases()` method
	# exists, call it to get an Array of case Arrays, then run the test once per
	# case. Each case emits its own test_pass/fail with name `func[idx]` or
	# `func[label]` when using the named form ["label", [arg1, arg2, ...]].
	var cases_func: String = "get_%s_cases" % func_name
	if instance.has_method(cases_func):
		var cases: Array = instance.call(cases_func)
		for idx in cases.size():
			var raw: Variant = cases[idx]
			var case_args: Array
			var case_name: String
			# Named case: ["label", [arg1, arg2, ...]] — label becomes the name.
			if raw is Array and raw.size() == 2 and raw[0] is String and raw[1] is Array:
				case_name = "%s[%s]" % [func_name, raw[0]]
				case_args = raw[1]
			else:
				case_name = "%s[%d]" % [func_name, idx]
				case_args = raw if raw is Array else [raw]
			await _run_case_on_instance(instance, script_path, func_name, case_name, case_args)
		_send({"event": "test_batch_done"})
		_release_test_instance(instance, instance is Node)
		return
	# Lazy container: only create one if the test class extends Node (so
	# its `_ready()` can fire) or if the test calls `Gdt.add_child_autofree`
	# during execution. Pure-RefCounted tests that don't touch the tree
	# pay zero overhead (no container, no post-test process_frame await).
	_current_test_file = script_path
	var instance_in_tree: bool = instance is Node
	if instance_in_tree:
		_ensure_container()
		_current_container.add_child(instance)
	if instance.has_method("before_each"):
		await instance.call("before_each")
	elif instance.has_method("before_test"):
		await instance.call("before_test")
	_fixture_before_each()
	_snapshot_orphan_baseline()
	_snapshot_memory_baseline()
	_send({"event": "test_start", "name": func_name})
	var t_start := Time.get_ticks_usec()
	# `await instance.call()` handles both sync (returns immediately) and
	# async (waits for coroutine completion). Validated in
	# spike/q6_coroutine_return.gd against Godot 4.6.2.
	await instance.call(func_name)
	var t_end := Time.get_ticks_usec()
	if instance.has_method("after_each"):
		await instance.call("after_each")
	elif instance.has_method("after_test"):
		await instance.call("after_test")
	_fixture_after_each()
	_restore_singletons()
	_cleanup_mocks()
	_cleanup_watchers()
	_reset_physics_delta_if_needed()
	_clear_push_captures()
	_cleanup_autorefs()
	var heap_delta := current_heap_delta()
	var us: int = t_end - t_start
	if _skip_msg != "":
		_send({"event": "test_skip", "name": func_name, "message": _skip_msg})
	elif _pending_msg != "":
		_send({"event": "test_pending", "name": func_name, "message": _pending_msg})
	elif _current_failures.is_empty():
		_send({"event": "test_pass", "name": func_name, "us": us, "heap_delta_bytes": heap_delta})
	else:
		_send({
			"event": "test_fail",
			"name": func_name,
			"us": us,
			"failures": _current_failures.duplicate(),
			"heap_delta_bytes": heap_delta,
		})
	# Tear down the container only if one was actually created. Pure
	# RefCounted tests skip both the queue_free and the process_frame
	# await — no overhead added to existing micro-tests.
	if _current_container != null:
		var c := _current_container
		_current_container = null
		c.queue_free()
		await process_frame
	# RefCounted instances drop with the local going out of scope; Node
	# instances were owned by the container and are now freed cascadewise.
	# Only Object subclasses need explicit free (suite instances are kept).
	# Guard: Node children freed by cascade above are no longer valid.
	if is_instance_valid(instance):
		_release_test_instance(instance, instance_in_tree)

# Run one parameterized case on a pre-created instance. Called by _run_test
# when a companion get_<func>_cases() method is present. Instance lifetime
# is managed by the caller.
func _run_case_on_instance(instance: Object, script_path: String, func_name: String, case_name: String, case_args: Array) -> void:
	_current_failures.clear()
	_pending_msg = ""
	_skip_msg = ""
	if instance.has_method("before_each"):
		await instance.call("before_each")
	_snapshot_orphan_baseline()
	_snapshot_memory_baseline()
	_send({"event": "test_start", "name": case_name})
	var t_start := Time.get_ticks_usec()
	await instance.callv(func_name, case_args)
	var t_end := Time.get_ticks_usec()
	if instance.has_method("after_each"):
		await instance.call("after_each")
	_restore_singletons()
	_cleanup_mocks()
	_cleanup_watchers()
	_reset_physics_delta_if_needed()
	_clear_push_captures()
	_cleanup_autorefs()
	var heap_delta := current_heap_delta()
	var us: int = t_end - t_start
	if _skip_msg != "":
		_send({"event": "test_skip", "name": case_name, "message": _skip_msg})
	elif _pending_msg != "":
		_send({"event": "test_pending", "name": case_name, "message": _pending_msg})
	elif _current_failures.is_empty():
		_send({"event": "test_pass", "name": case_name, "us": us, "heap_delta_bytes": heap_delta})
	else:
		_send({"event": "test_fail", "name": case_name, "us": us, "failures": _current_failures.duplicate(), "heap_delta_bytes": heap_delta})
	if _current_container != null:
		var c := _current_container
		_current_container = null
		c.queue_free()
		await process_frame

# Run a single test function on an inner-class instance (describe_ block).
# Frees the instance when done.
func _run_describe_test(instance: Object, func_name: String, display_name: String) -> void:
	_current_failures.clear()
	_pending_msg = ""
	_skip_msg = ""
	if not instance.has_method(func_name):
		_send({"event": "test_error", "name": display_name, "message": "no such func `%s` in inner class" % func_name})
		_free_instance(instance)
		return
	if instance.has_method("before_each"):
		await instance.call("before_each")
	_snapshot_orphan_baseline()
	_snapshot_memory_baseline()
	_send({"event": "test_start", "name": display_name})
	var t_start := Time.get_ticks_usec()
	await instance.call(func_name)
	var t_end := Time.get_ticks_usec()
	if instance.has_method("after_each"):
		await instance.call("after_each")
	_restore_singletons()
	_cleanup_mocks()
	_cleanup_watchers()
	_reset_physics_delta_if_needed()
	_clear_push_captures()
	_cleanup_autorefs()
	var heap_delta := current_heap_delta()
	var us: int = t_end - t_start
	if _skip_msg != "":
		_send({"event": "test_skip", "name": display_name, "message": _skip_msg})
	elif _pending_msg != "":
		_send({"event": "test_pending", "name": display_name, "message": _pending_msg})
	elif _current_failures.is_empty():
		_send({"event": "test_pass", "name": display_name, "us": us, "heap_delta_bytes": heap_delta})
	else:
		_send({"event": "test_fail", "name": display_name, "us": us, "failures": _current_failures.duplicate(), "heap_delta_bytes": heap_delta})
	if _current_container != null:
		var c := _current_container
		_current_container = null
		c.queue_free()
		await process_frame
	_free_instance(instance)

func _run_file(script_path: String) -> void:
	if script_path == "":
		_send({"event": "test_error", "name": "<file>", "message": "missing script"})
		return
	var resource: Resource = load(script_path)
	if resource == null or not (resource is GDScript):
		_send({"event": "test_error", "name": script_path, "message": "could not load script: %s" % script_path})
		return
	var gds: GDScript = resource
	if not gds.can_instantiate():
		_send({"event": "test_error", "name": script_path, "message": "script cannot be instantiated (parse error?): %s" % script_path})
		return
	var probe: Object = gds.new()
	var methods: Array[String] = []
	for m in probe.get_method_list():
		var name: String = m.get("name", "")
		if name.begins_with("test_"):
			methods.append(name)
	var has_suite_hooks: bool = probe.has_method("before_all") or probe.has_method("after_all") \
		or probe.has_method("before") or probe.has_method("after")
	if has_suite_hooks:
		# Shared instance path: one object for the whole file.
		# Node instances are parented to root so they survive between tests
		# (per-test containers are still created for add_child_autofree nodes).
		if probe is Node:
			root.add_child(probe)
		if probe.has_method("before_all"):
			await probe.call("before_all")
		elif probe.has_method("before"):
			await probe.call("before")
		for m in methods:
			await _run_test_on_instance(probe, script_path, m)
		if probe.has_method("after_all"):
			await probe.call("after_all")
		elif probe.has_method("after"):
			await probe.call("after")
		_free_instance(probe)
	else:
		_free_instance(probe)
		for m in methods:
			await _run_test(script_path, m)

# Run a single test method on an already-created instance. Used by _run_file
# when before_all/after_all are present. Does not instantiate or free the
# instance — the caller owns its lifetime.
func _run_test_on_instance(instance: Object, script_path: String, func_name: String) -> void:
	_current_failures.clear()
	_pending_msg = ""
	_skip_msg = ""
	if not instance.has_method(func_name):
		_send({"event": "test_error", "name": func_name, "message": "no such func `%s` in %s" % [func_name, script_path]})
		return
	if instance.has_method("before_each"):
		await instance.call("before_each")
	elif instance.has_method("before_test"):
		await instance.call("before_test")
	_snapshot_orphan_baseline()
	_snapshot_memory_baseline()
	_send({"event": "test_start", "name": func_name})
	var t_start := Time.get_ticks_usec()
	await instance.call(func_name)
	var t_end := Time.get_ticks_usec()
	if instance.has_method("after_each"):
		await instance.call("after_each")
	elif instance.has_method("after_test"):
		await instance.call("after_test")
	_restore_singletons()
	_cleanup_mocks()
	_cleanup_watchers()
	_reset_physics_delta_if_needed()
	_clear_push_captures()
	_cleanup_autorefs()
	var heap_delta := current_heap_delta()
	var us: int = t_end - t_start
	if _skip_msg != "":
		_send({"event": "test_skip", "name": func_name, "message": _skip_msg})
	elif _pending_msg != "":
		_send({"event": "test_pending", "name": func_name, "message": _pending_msg})
	elif _current_failures.is_empty():
		_send({"event": "test_pass", "name": func_name, "us": us, "heap_delta_bytes": heap_delta})
	else:
		_send({"event": "test_fail", "name": func_name, "us": us, "failures": _current_failures.duplicate(), "heap_delta_bytes": heap_delta})
	if _current_container != null:
		var c := _current_container
		_current_container = null
		c.queue_free()
		await process_frame

# ── Signal assertions ────────────────────────────────────────────────────

func _node_desc(node: Object) -> String:
	var base := "%s#%d" % [node.get_class(), node.get_instance_id()]
	if node is Node and node.is_inside_tree():
		return "<%s @ %s>" % [base, str(node.get_path())]
	return "<%s>" % base

# Returns a Callable that accepts the same number of args as `sig` on `node`
# and calls `on_fire` with the captured args as an Array. Covers 0–4 arg
# signals; falls back to 0-arg for unknown counts.
func _make_signal_cb(on_fire: Callable, node: Object, sig: String) -> Callable:
	var arg_count := 0
	for s in node.get_signal_list():
		if s["name"] == sig:
			arg_count = s["args"].size()
			break
	match arg_count:
		0: return func(): on_fire.call([])
		1: return func(a0): on_fire.call([a0])
		2: return func(a0, a1): on_fire.call([a0, a1])
		3: return func(a0, a1, a2): on_fire.call([a0, a1, a2])
		_: return func(a0, a1, a2, a3): on_fire.call([a0, a1, a2, a3])

# Awaits `sig` on `node` for up to `timeout_ms` milliseconds.
# Records a failure into _current_failures if the signal does not fire in time.
# Assert signal emitted exactly `expected_count` times within timeout_ms.
func watch_signal_count(node: Object, sig: String, expected_count: int, timeout_ms: float = 1000.0) -> void:
	var state := {"count": 0}  # Dict so lambda mutation propagates back (value types are captured by copy)
	var cb := _make_signal_cb(func(_args): state.count += 1, node, sig)
	node.connect(sig, cb)
	var timer := create_timer(timeout_ms / 1000.0)
	var frames := 0
	while state.count < expected_count and timer.time_left > 0.0:
		await process_frame
		frames += 1
	if node.is_connected(sig, cb):
		node.disconnect(sig, cb)
	if state.count != expected_count:
		_current_failures.append(
			"signal '%s' on %s: expected %d emission(s), got %d (waited %.0fms, %d frames)" % [sig, _node_desc(node), expected_count, state.count, timeout_ms, frames]
		)

# Assert signal emitted with args matching `expected_args` within timeout_ms.
func watch_signal_with_args(node: Object, sig: String, expected_args: Array, timeout_ms: float = 1000.0) -> void:
	var state := {"fired": false, "args": []}  # Dict so lambda mutations propagate back (value types are captured by copy)
	var cb := _make_signal_cb(func(args): state.fired = true; state.args = args, node, sig)
	node.connect(sig, cb, CONNECT_ONE_SHOT)
	var timer := create_timer(timeout_ms / 1000.0)
	var frames := 0
	while not state.fired and timer.time_left > 0.0:
		await process_frame
		frames += 1
	if not state.fired:
		if node.is_connected(sig, cb):
			node.disconnect(sig, cb)
		_current_failures.append(
			"signal '%s' was not emitted on %s within %.0fms (waited %d frames)" % [sig, _node_desc(node), timeout_ms, frames]
		)
		return
	if state.args != expected_args:
		_current_failures.append(
			"signal '%s' on %s: emitted with %s, expected %s" % [sig, _node_desc(node), str(state.args), str(expected_args)]
		)

func watch_signal(node: Object, sig: String, timeout_ms: float = 1000.0, context_fn: Callable = Callable()) -> void:
	var state := {"fired": false}  # Dict so lambda mutation propagates back (value types are captured by copy)
	var cb := func(_a = null, _b = null, _c = null, _d = null): state.fired = true
	node.connect(sig, cb, CONNECT_ONE_SHOT)
	var timer := create_timer(timeout_ms / 1000.0)
	var frames := 0
	while not state.fired:
		await process_frame
		frames += 1
		if timer.time_left <= 0.0:
			break
	if not state.fired:
		node.disconnect(sig, cb)
		var msg := "signal '%s' was not emitted on %s within %.0fms\n  Waited: %d process frames" % [sig, _node_desc(node), timeout_ms, frames]
		if context_fn.is_valid():
			msg += "\n  Context: %s" % str(context_fn.call())
		_current_failures.append(msg)

# Waits for `timeout_ms` milliseconds and fails if `sig` fires during that window.
func watch_no_signal(node: Object, sig: String, timeout_ms: float = 200.0) -> void:
	var state := {"fired": false}  # Dict so lambda mutation propagates back (value types are captured by copy)
	var cb := func(_a = null, _b = null, _c = null, _d = null): state.fired = true
	node.connect(sig, cb, CONNECT_ONE_SHOT)
	var timer := create_timer(timeout_ms / 1000.0)
	while not state.fired and timer.time_left > 0.0:
		await process_frame
	if state.fired:
		_current_failures.append("signal '%s' was unexpectedly emitted on %s" % [sig, _node_desc(node)])
	elif node.is_connected(sig, cb):
		node.disconnect(sig, cb)

func register_singleton_swap(name: String, real_node: Node, mock_node: Node) -> void:
	_singleton_swaps[name] = {"real": real_node, "mock": mock_node}

func _restore_singletons() -> void:
	for sname in _singleton_swaps:
		var swap: Dictionary = _singleton_swaps[sname]
		root.remove_child(swap["mock"])
		swap["mock"].queue_free()
		swap["real"].name = sname
	_singleton_swaps.clear()

func register_mock(obj: Object) -> void:
	_mock_registry.append(obj)

func _cleanup_mocks() -> void:
	for obj in _mock_registry:
		if is_instance_valid(obj) and obj is Node and not obj.is_inside_tree():
			obj.free()
	_mock_registry.clear()

func next_emission_seq() -> int:
	_emission_counter += 1
	return _emission_counter

func register_watcher_keyed(obj: Object, sig: String, w: Object) -> void:
	_watcher_registry.append(w)
	_watcher_map["%d|%s" % [obj.get_instance_id(), sig]] = w

func _find_watcher(obj: Object, sig: String) -> Object:
	return _watcher_map.get("%d|%s" % [obj.get_instance_id(), sig], null)

func _cleanup_watchers() -> void:
	for w in _watcher_registry:
		if is_instance_valid(w):
			w._disconnect()
	_watcher_registry.clear()
	_watcher_map.clear()

func set_physics_delta(seconds: float) -> void:
	if _original_physics_tps < 0:
		_original_physics_tps = Engine.physics_ticks_per_second
	Engine.physics_ticks_per_second = maxi(1, roundi(1.0 / seconds))

func reset_physics_delta() -> void:
	if _original_physics_tps >= 0:
		Engine.physics_ticks_per_second = _original_physics_tps
		_original_physics_tps = -1

func _reset_physics_delta_if_needed() -> void:
	if _original_physics_tps >= 0:
		Engine.physics_ticks_per_second = _original_physics_tps
		_original_physics_tps = -1

func _snapshot_orphan_baseline() -> void:
	_orphan_baseline = int(Performance.get_monitor(Performance.OBJECT_ORPHAN_NODE_COUNT))

func capture_push_error(msg: String) -> void:
	_captured_errors.append(msg)

func capture_push_warning(msg: String) -> void:
	_captured_warnings.append(msg)

func _clear_push_captures() -> void:
	_captured_errors.clear()
	_captured_warnings.clear()

func _push_msg_matches(actual: String, pattern: String) -> bool:
	if pattern.begins_with("/") and pattern.ends_with("/") and pattern.length() > 2:
		var re := RegEx.create_from_string(pattern.substr(1, pattern.length() - 2))
		return re != null and re.search(actual) != null
	return actual == pattern

func assert_push_error(text: String, msg: String = "") -> bool:
	for e in _captured_errors:
		if _push_msg_matches(e, text):
			return true
	var m: String = msg if msg != "" else \
		"expected push_error matching '%s' but captured: %s" % [text, str(_captured_errors)]
	return expect(false, m)

func assert_push_warning(text: String, msg: String = "") -> bool:
	for w in _captured_warnings:
		if _push_msg_matches(w, text):
			return true
	var m: String = msg if msg != "" else \
		"expected push_warning matching '%s' but captured: %s" % [text, str(_captured_warnings)]
	return expect(false, m)

func assert_no_push_error(msg: String = "") -> bool:
	if _captured_errors.is_empty():
		return true
	var m: String = msg if msg != "" else \
		"expected no push_error() calls, but captured: %s" % str(_captured_errors)
	return expect(false, m)

func assert_no_new_orphans(msg: String = "") -> bool:
	if _orphan_baseline < 0:
		return expect(false, "assert_no_new_orphans: baseline not set (must be called after before_each)")
	var current: int = int(Performance.get_monitor(Performance.OBJECT_ORPHAN_NODE_COUNT))
	var delta: int = current - _orphan_baseline
	if delta <= 0:
		return true
	var m: String = msg if msg != "" else \
		"assert_no_new_orphans: %d new orphan node(s) detected" % delta
	return expect(false, m)

# ── Memory tracking + autoref scoping ──────────────────────────────

func _snapshot_memory_baseline() -> void:
	_memory_baseline = int(Performance.get_monitor(Performance.MEMORY_STATIC))

func current_heap_delta() -> int:
	if _memory_baseline < 0:
		return 0
	return int(Performance.get_monitor(Performance.MEMORY_STATIC)) - _memory_baseline

func autoref(obj: Object) -> Object:
	_autoref_registry.append(obj)
	return obj

func _cleanup_autorefs() -> void:
	for obj in _autoref_registry:
		if is_instance_valid(obj):
			if obj is Node:
				if obj.is_inside_tree():
					obj.queue_free()
				else:
					obj.free()
			elif not (obj is RefCounted):
				obj.free()
	_autoref_registry.clear()

# ── Property / export assertions ───────────────────────────────────

func _find_property(obj: Object, prop_name: String) -> Dictionary:
	for prop in obj.get_property_list():
		if prop["name"] == prop_name:
			return prop
	return {}

func assert_exports(obj: Object, prop_name: String, type: int, msg: String = "") -> bool:
	var prop: Dictionary = _find_property(obj, prop_name)
	if prop.is_empty():
		return expect(false, msg if msg != "" else \
			"no property '%s' found on %s" % [prop_name, obj.get_class()])
	if not (prop["usage"] & PROPERTY_USAGE_EDITOR):
		return expect(false, msg if msg != "" else \
			"property '%s' exists but is not @export-ed" % prop_name)
	if prop["type"] != type:
		return expect(false, msg if msg != "" else \
			"@export '%s' has type %d, expected %d" % [prop_name, prop["type"], type])
	return true

func assert_property(obj: Object, prop_name: String, default_value: Variant, msg: String = "") -> bool:
	var script: GDScript = obj.get_script() as GDScript
	var fresh: Object = null
	if script != null and script.can_instantiate():
		fresh = script.new()
	elif ClassDB.can_instantiate(obj.get_class()):
		fresh = ClassDB.instantiate(obj.get_class())
	if fresh == null:
		return expect(false, "assert_property: could not instantiate fresh %s" % obj.get_class())
	var actual: Variant = fresh.get(prop_name)
	if not (fresh is RefCounted):
		if fresh is Node:
			fresh.queue_free()
		else:
			fresh.free()
	var m: String = msg if msg != "" else \
		"property '%s': default is %s, expected %s" % [prop_name, str(actual), str(default_value)]
	return expect(actual == default_value, m)

func _probe_value_for(v: Variant) -> Variant:
	match typeof(v):
		TYPE_INT: return int(v) + 99999
		TYPE_FLOAT: return float(v) + 99999.0
		TYPE_BOOL: return not bool(v)
		TYPE_STRING: return "__gdt_probe__"
		_: return null

func assert_readonly_property(obj: Object, prop_name: String, msg: String = "") -> bool:
	var original: Variant = obj.get(prop_name)
	var probe: Variant = _probe_value_for(original)
	obj.set(prop_name, probe)
	var after: Variant = obj.get(prop_name)
	obj.set(prop_name, original)
	if after == original:
		return true
	var m: String = msg if msg != "" else \
		"property '%s' is not read-only (value changed after set)" % prop_name
	return expect(false, m)

func assert_property_with_range(obj: Object, prop_name: String, min_val: float, max_val: float, msg: String = "") -> bool:
	var prop: Dictionary = _find_property(obj, prop_name)
	if prop.is_empty():
		return expect(false, msg if msg != "" else \
			"no property '%s' found on %s" % [prop_name, obj.get_class()])
	if prop["hint"] != PROPERTY_HINT_RANGE:
		return expect(false, msg if msg != "" else \
			"property '%s' does not have @export_range (hint=%d)" % [prop_name, prop["hint"]])
	var parts: PackedStringArray = prop["hint_string"].split(",")
	if parts.size() < 2:
		return expect(false, "property '%s' range hint_string malformed: '%s'" % [prop_name, prop["hint_string"]])
	var actual_min := float(parts[0])
	var actual_max := float(parts[1])
	var m: String = msg if msg != "" else \
		"property '%s' range: expected [%s, %s], got [%s, %s]" % [prop_name, min_val, max_val, actual_min, actual_max]
	return expect(actual_min == min_val and actual_max == max_val, m)

func _free_instance(instance: Object) -> void:
	if instance is RefCounted:
		return
	if instance is Node:
		instance.queue_free()
		return
	instance.free()

# ── Plugin system ───────────────────────────────────────────────────

func _load_plugins() -> void:
	var env: String = OS.get_environment("GDTEST_PLUGINS")
	if env.is_empty():
		return
	for path in env.split(","):
		path = path.strip_edges()
		if path.is_empty():
			continue
		var res: Resource = load(path)
		if not (res is GDScript):
			push_warning("gdtest: plugin %s is not a GDScript — skipped" % path)
			continue
		var gds: GDScript = res
		if not gds.can_instantiate():
			push_warning("gdtest: plugin %s cannot be instantiated — skipped" % path)
			continue
		var inst: Object = gds.new()
		if inst is Gdt.GdtReporter:
			_reporter_plugins.append(inst)
		elif inst is Gdt.GdtFixture:
			_fixture_plugins.append(inst)
		else:
			push_warning("gdtest: plugin %s does not extend GdtReporter or GdtFixture — skipped" % path)

func _notify_reporters(event: Dictionary) -> void:
	if _reporter_plugins.is_empty():
		return
	var ev: String = event.get("event", "")
	var file: String = _current_test_file
	var name: String = event.get("name", "")
	for rp in _reporter_plugins:
		match ev:
			"test_start":
				rp.on_test_start(file, name)
			"test_pass":
				rp.on_test_pass(file, name, event.get("us", 0))
			"test_fail":
				rp.on_test_fail(file, name, event.get("us", 0), event.get("failures", []))
			"test_skip":
				rp.on_test_skip(file, name, event.get("message", ""))
			"test_error":
				rp.on_test_error(file, name, event.get("message", ""))

func _fixture_before_each() -> void:
	for fp in _fixture_plugins:
		fp.before_each()

func _fixture_after_each() -> void:
	for fp in _fixture_plugins:
		fp.after_each()

# A test's result must account for the script errors raised since it started.
func _with_script_errors(obj: Dictionary) -> Dictionary:
	match obj.get("event"):
		"test_start":
			_script_error_logger.take_errors()
		"test_pass":
			var errors := _script_error_logger.take_errors()
			if not errors.is_empty():
				return {
					"event": "test_error",
					"name": obj["name"],
					"message": "script error: %s" % " | ".join(errors),
				}
		"test_fail":
			for error in _script_error_logger.take_errors():
				obj["failures"].append("script error: %s" % error)
	return obj

func _send(obj: Dictionary) -> void:
	obj = _with_script_errors(obj)
	_notify_reporters(obj)
	print(OUT_PREFIX + JSON.stringify(obj))

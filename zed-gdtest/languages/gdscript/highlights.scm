; ==============================================================================
; gdtest-specific highlights (must come before GDScript base — first match wins)
; ==============================================================================

; Disabled test functions (xtest_*) rendered like comments
(function_definition
  name: (name) @comment
  (#match? @comment "^xtest_"))

; Test, focused-test, and benchmark function names
(function_definition
  name: (name) @tag
  (#match? @tag "^(test_|ftest_|bench_)"))

; Lifecycle hooks
(function_definition
  name: (name) @keyword
  (#any-of? @keyword "before_each" "after_each" "before_all" "after_all"))

; assert_eq(...) / expect_*(...) — direct calls
(call
  (identifier) @function.builtin
  (#match? @function.builtin "^(assert_|expect_)"))

; Gdt.assert_eq(...) / Gdt.expect_*(...) — attribute calls
(attribute
  (attribute_call
    (identifier) @function.builtin
    (#match? @function.builtin "^(assert_|expect_)")))

; mock(…) / spy(…) / stub(…) / partial_mock(…) — direct calls
(call
  (identifier) @function.builtin
  (#any-of? @function.builtin "mock" "spy" "stub" "partial_mock" "mock_singleton" "watch_signal"))

; Gdt.mock(…) — attribute calls
(attribute
  (attribute_call
    (identifier) @function.builtin
    (#any-of? @function.builtin "mock" "spy" "stub" "partial_mock" "mock_singleton" "watch_signal")))

; pending(…) / skip(…) / fail(…) / describe(…) — direct calls
(call
  (identifier) @keyword
  (#any-of? @keyword "pending" "skip" "fail" "describe"))

; Gdt.pending(…) / Gdt.skip(…) — attribute calls
(attribute
  (attribute_call
    (identifier) @keyword
    (#any-of? @keyword "pending" "skip" "fail" "describe")))

; forall(…) / gen_*(…) / wait_*(…) — direct calls
(call
  (identifier) @function.builtin
  (#match? @function.builtin "^(forall|gen_|wait_)"))

; Gdt.forall(…) / Gdt.gen_*(…) — attribute calls
(attribute
  (attribute_call
    (identifier) @function.builtin
    (#match? @function.builtin "^(forall|gen_|wait_)")))

; ==============================================================================
; GDScript base highlights
; ==============================================================================

; Control flow
[
  "if"
  "elif"
  "else"
  "for"
  "while"
  "match"
  "break"
  "continue"
  "pass"
  "return"
  "when"
] @keyword.control

; Declaration keywords
[
  "class"
  "class_name"
  "extends"
  "func"
  "enum"
  "signal"
  "var"
  "const"
  "static"
  "await"
  "breakpoint"
  "in"
  "not"
  "and"
  "or"
  "is"
  "as"
] @keyword

; Annotations (@export, @onready, @tool, etc.)
(annotation "@" @punctuation.special)
(annotation
  (identifier) @attribute)

; Class names
(class_definition
  name: (name) @type)

(class_name_statement
  name: (name) @type)

; Enum names and members
(enum_definition
  name: (name) @type)

(enumerator
  left: (identifier) @constant)

; Function definitions (generic — after gdtest-specific above)
(function_definition
  name: (name) @function)

(lambda
  name: (name) @function)

; Function calls — plain identifier
(call
  (identifier) @function)

; Function calls — attribute (e.g. self.my_method())
(attribute
  (attribute_call
    (identifier) @function.method))

; Base call (.super_method())
(base_call
  (identifier) @function.method)

; Variable declarations
(variable_statement
  name: (name) @variable)

(const_statement
  name: (name) @constant)

; Parameters
(typed_parameter
  (identifier) @variable.parameter)

(default_parameter
  (identifier) @variable.parameter)

(typed_default_parameter
  (identifier) @variable.parameter)

(parameters
  (identifier) @variable.parameter)

; For loop variable
(for_statement
  left: (identifier) @variable)

; Signal names
(signal_statement
  name: (name) @variable.special)

; Built-in type names
((identifier) @type.builtin
 (#any-of? @type.builtin
  "int" "float" "bool" "String" "Vector2" "Vector2i" "Vector3" "Vector3i"
  "Vector4" "Vector4i" "Rect2" "Rect2i" "Color" "Array" "Dictionary"
  "NodePath" "Callable" "Signal" "RID" "Object" "Basis" "Transform2D"
  "Transform3D" "Plane" "Quaternion" "AABB" "Projection" "StringName"
  "PackedByteArray" "PackedInt32Array" "PackedInt64Array"
  "PackedFloat32Array" "PackedFloat64Array" "PackedStringArray"
  "PackedVector2Array" "PackedVector3Array" "PackedColorArray"
  "Variant" "Error" "void"))

; Built-in constants
((identifier) @constant.builtin
 (#any-of? @constant.builtin "PI" "TAU" "INF" "NAN" "OK" "FAILED"))

; self / super
((identifier) @variable.builtin
 (#any-of? @variable.builtin "self" "super"))

; General identifier fallback
(identifier) @variable

; Literals
(string) @string
(string_name) @string.special
(node_path) @string.special
(escape_sequence) @string.escape

(get_node "$" @punctuation.special)
(get_node "%" @punctuation.special)

(integer) @number
(float) @number

(true) @boolean
(false) @boolean
(null) @constant.builtin

; Comments
(comment) @comment
(region_start) @comment
(region_end) @comment

; Operators
[
  "+"  "-"  "*"  "/"  "**"  "%"
  "&"  "|"  "^"  "~"  "<<"  ">>"
  "==" "!=" "<"  ">"  "<="  ">="
  "="  "+=" "-=" "*=" "/="  "**=" "%="
  "&=" "|=" "^=" "<<=" ">>="
  "!"  "&&" "||"
  "->"
] @operator

; Punctuation
[
  "("  ")"
  "["  "]"
  "{"  "}"
] @punctuation.bracket

[
  ","
  "."
  ":"
  ";"
] @punctuation.delimiter

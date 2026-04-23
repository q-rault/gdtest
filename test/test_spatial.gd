extends GdTest

func test_vec2_approx_identical() -> void:
	assert_vec2_approx(Vector2(1.0, 2.0), Vector2(1.0, 2.0))

func test_vec2_approx_within_tolerance() -> void:
	assert_vec2_approx(Vector2(1.0, 2.0), Vector2(1.0, 2.0 + 1e-7))

func test_vec3_approx_identical() -> void:
	assert_vec3_approx(Vector3.ONE, Vector3.ONE)

func test_vec3_approx_within_tolerance() -> void:
	assert_vec3_approx(Vector3.ONE, Vector3(1.0, 1.0, 1.0 + 1e-7))

func test_vec4_approx_identical() -> void:
	assert_vec4_approx(Vector4(1, 2, 3, 4), Vector4(1, 2, 3, 4))

func test_vec4_approx_within_tolerance() -> void:
	assert_vec4_approx(Vector4(1, 2, 3, 4), Vector4(1, 2, 3, 4.0 + 1e-7))

func test_color_approx_identical() -> void:
	assert_color_approx(Color(1, 0, 0, 1), Color(1, 0, 0, 1))

func test_color_approx_within_tolerance() -> void:
	assert_color_approx(Color(1, 0, 0, 1), Color(1, 0, 0, 1.0 + 1e-7))

func test_rect2_approx_identical() -> void:
	assert_rect2_approx(Rect2(0, 0, 10, 10), Rect2(0, 0, 10, 10))

func test_rect2_approx_within_tolerance() -> void:
	assert_rect2_approx(Rect2(0, 0, 10, 10), Rect2(0, 1e-7, 10, 10))

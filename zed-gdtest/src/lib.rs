use zed_extension_api as zed;

struct GdtestExtension;

impl zed::Extension for GdtestExtension {
    fn new() -> Self {
        GdtestExtension
    }
}

zed::register_extension!(GdtestExtension);

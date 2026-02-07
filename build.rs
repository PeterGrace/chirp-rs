// Build script for Qt GUI
// This compiles any C++ code needed for Qt integration

fn main() {
    // Only build Qt components when GUI feature is enabled
    #[cfg(feature = "gui")]
    {
        cpp_build::Config::new()
            .build("src/gui/qt_gui.rs");
    }
}

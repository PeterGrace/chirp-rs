// Build script for Qt GUI
// This compiles any C++ code needed for Qt integration

fn main() {
    // Only build Qt components when GUI feature is enabled
    #[cfg(feature = "gui")]
    {
        // Use Qt6 since qmetaobject pulls in Qt6Core
        let qt_include_path = "/usr/include/x86_64-linux-gnu/qt6";

        cpp_build::Config::new()
            .include(qt_include_path)
            .include(format!("{}/QtCore", qt_include_path))
            .include(format!("{}/QtGui", qt_include_path))
            .include(format!("{}/QtWidgets", qt_include_path))
            .flag("-fPIC")
            .flag("-std=c++17")
            .build("src/gui/qt_gui.rs");

        // Link Qt6 libraries
        println!("cargo:rustc-link-lib=Qt6Widgets");
        println!("cargo:rustc-link-lib=Qt6Gui");
        println!("cargo:rustc-link-lib=Qt6Core");
    }
}

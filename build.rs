// Build script for Qt GUI
// This compiles any C++ code needed for Qt integration

fn main() {
    // Only build Qt components when GUI feature is enabled
    #[cfg(feature = "gui")]
    {
        use std::{env, path::PathBuf};
        // Use Qt6 since qmetaobject pulls in Qt6Core
        let target = env::var("TARGET").unwrap_or_default();
        let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_default(); // "windows", "linux", ...
        let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default(); // "msvc", "gnu", ...

        let (qt_include_root, qt_lib_dir) = if target_os == "windows" {
            // Example: QTDIR=C:\Qt\6.6.2\msvc2019_64
            let qtdir = env::var("QTDIR")
                .or_else(|_| env::var("QT6_DIR"))
                .expect("On Windows, set QTDIR (or QT6_DIR) to your Qt 6 MSVC install directory (it should contain include/ and lib/).");

            let base = PathBuf::from(qtdir);
            (base.join("include"), Some(base.join("lib")))
        } else {
            // Linux (or other Unix): allow override, otherwise try common paths
            let candidates = [
                env::var_os("QT_INCLUDE_PATH").map(PathBuf::from),
                Some(PathBuf::from("/usr/include/x86_64-linux-gnu/qt6")),
                Some(PathBuf::from("/usr/include/qt6")),
            ];

            let include_root = candidates
                .into_iter()
                .flatten()
                .find(|p| p.exists())
                .expect(
                    "Qt include path not found. Set QT_INCLUDE_PATH to your Qt6 include directory.",
                );

            (include_root, None)
        };

        let mut cfg = cpp_build::Config::new();
        cfg.include(&qt_include_root)
            .include(qt_include_root.join("QtCore"))
            .include(qt_include_root.join("QtGui"))
            .include(qt_include_root.join("QtWidgets"));

        // C++ standard flags (pick the one the compiler understands)
        cfg.flag_if_supported("-std=c++17");
        cfg.flag_if_supported("/std:c++17");

        // Non-Windows often needs PIC
        if target_os != "windows" {
            cfg.flag_if_supported("-fPIC");
        }

        // MSVC-specific niceties
        if target_os == "windows" && target_env == "msvc" {
            cfg.flag_if_supported("/EHsc");
            cfg.flag_if_supported("/permissive-");
        }

        cfg.build("src/gui/qt_gui.rs");

        // Tell rustc where Qt .lib files live on Windows
        if let Some(lib_dir) = qt_lib_dir {
            println!("cargo:rustc-link-search=native={}", lib_dir.display());
        }

        // Link Qt6 libraries (MSVC: these correspond to Qt6*.lib import libs)
        println!("cargo:rustc-link-lib=Qt6Widgets");
        println!("cargo:rustc-link-lib=Qt6Gui");
        println!("cargo:rustc-link-lib=Qt6Core");
    }
}

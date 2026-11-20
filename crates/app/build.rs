//! Registers the app's own QML module and runs the cxx-qt bridge through the
//! macro.
//!
//! blockstitch's `com.blockworked.Blockstitch` module comes from the
//! `blockstitch-qml` dependency, which declares it in its own build script -
//! cxx-qt-build picks a dependency's QML module up automatically, so it only has
//! to be named here as a dependency of ours for the import to resolve.

use cxx_qt_build::{CxxQtBuilder, QmlModule};
use std::path::PathBuf;

#[path = "build_support/qwindowkit.rs"]
mod qwindowkit;

/// Puts the app icon and version information into `stitchcraft.exe`.
///
/// Windows reads an app's icon out of the executable itself - Explorer, the
/// taskbar, the Start menu and the installer's shortcuts all do. Qt also looks
/// for a resource named `IDI_ICON1` when it registers its window class, which is
/// what puts the icon on the app's own windows. The version block is what the
/// file's Properties dialog shows.
///
/// Does nothing when not building for Windows.
fn embed_windows_resources() {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("set by cargo"));
    // Forward slashes, and no `\\?\` prefix (which `canonicalize` would add): the
    // resource compiler chokes on both when they are inside a string literal.
    let icon = manifest_dir.join("../../res/icons/stitchcraft.ico");
    assert!(icon.exists(), "app icon not found at {}", icon.display());
    let icon = icon.to_string_lossy().replace('\\', "/");
    println!("cargo:rerun-if-changed={icon}");

    let version = std::env::var("CARGO_PKG_VERSION").expect("set by cargo");
    // `0.1.0-dev` -> 0,1,0,0: the numeric FILEVERSION cannot hold a suffix.
    let mut parts = version
        .split(|c: char| !c.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .map(|part| part.parse::<u16>().unwrap_or(0));
    let numeric = format!(
        "{},{},{},0",
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0),
        parts.next().unwrap_or(0)
    );

    let rc = format!(
        r#"IDI_ICON1 ICON "{icon}"

1 VERSIONINFO
FILEVERSION {numeric}
PRODUCTVERSION {numeric}
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904b0"
    BEGIN
      VALUE "FileDescription", "Stitchcraft"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "stitchcraft"
      VALUE "OriginalFilename", "stitchcraft.exe"
      VALUE "ProductName", "Stitchcraft"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#
    );
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("set by cargo")).join("stitchcraft.rc");
    std::fs::write(&out, rc).expect("write the resource script");

    // Required, not optional: a failure here would otherwise ship an exe with the
    // default icon and no version info, and nobody would notice until install.
    embed_resource::compile(&out, embed_resource::NONE)
        .manifest_required()
        .expect("compile the Windows resources");
}

/// QtCore/QtGui private headers, for the Wayland frame-margin hook in
/// `src/window.cpp`. Empty off Linux, or where Qt has none installed.
fn qt_private_includes() -> Vec<PathBuf> {
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("linux") {
        return Vec::new();
    }
    let Ok(qt) = qt_build_utils::QtBuild::new(vec!["Core".into(), "Gui".into()]) else {
        return Vec::new();
    };
    let version = qt.version().to_string();
    qt.include_paths()
        .into_iter()
        .filter_map(|dir| {
            let name = dir.file_name()?.to_str()?;
            (name == "QtCore" || name == "QtGui").then(|| dir.join(&version))
        })
        .filter(|dir| dir.exists())
        .collect()
}

fn main() {
    embed_windows_resources();
    let qwindowkit = qwindowkit::build();

    let builder = CxxQtBuilder::new_qml_module(
        QmlModule::new("com.blockworked.Stitchcraft")
            .version(1, 0)
            .depend("com.blockworked.Blockstitch")
            .qml_files([
                // Order does not matter to the module, but it reads as the
                // dependency order: theme, then rows, then the shell.
                "qml/Stitchcraft/BlockRows.qml",
                "qml/Stitchcraft/IconButton.qml",
                "qml/Stitchcraft/DiagnosticsBar.qml",
                "qml/Stitchcraft/TitleBarDrag.qml",
                "qml/Stitchcraft/WindowControls.qml",
                "qml/Stitchcraft/DevTools.qml",
                "qml/Stitchcraft/ConsolePanel.qml",
                "qml/Stitchcraft/DevDialogs.qml",
                "qml/Stitchcraft/ModSettingsDialog.qml",
                "qml/Stitchcraft/MakeBlockDialog.qml",
                "qml/Stitchcraft/Main.qml",
                "qml/Stitchcraft/Start.qml",
            ]),
    )
    .qrc("qml/controls.qrc")
    .file("src/bridge.rs")
    .file("src/fonts.rs")
    .file("src/window.rs")
    .qt_module("Quick")
    .qt_module("QuickControls2");
    for file in ["fonts.cpp", "fonts.h", "window.cpp", "window.h"] {
        println!("cargo:rerun-if-changed=src/{file}");
    }
    let private_includes = qt_private_includes();
    // SAFETY: only adds this crate's own C++ files and include directories.
    unsafe {
        builder
            .cc_builder(|cc| {
                cc.include("src");
                cc.include(qwindowkit.join("include/QWindowKit"));
                cc.define("QWK_QUICK_STATIC", None);
                cc.define("QWK_CORE_STATIC", None);
                cc.file("src/fonts.cpp");
                cc.file("src/window.cpp");
                for dir in &private_includes {
                    cc.include(dir);
                }
            })
            .build();
    }
    qwindowkit::link(&qwindowkit);
}

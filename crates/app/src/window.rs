//! The frameless window, the way Blockwork draws it: QWindowKit for moving,
//! snapping and the system menu, with the shadow, corners and buttons drawn in
//! QML. See `window.cpp` and `qml/window/WindowChrome.qml`.

#[cxx::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qqmlengine.h");
        type QQmlEngine = cxx_qt_lib::QQmlEngine;
        include!("window.h");

        fn stitchcraft_prepare_window(engine: Pin<&mut QQmlEngine>);
        fn stitchcraft_set_window_frame_margin(margin: i32);
    }
}

/// Call before the window is created.
pub fn prepare(engine: std::pin::Pin<&mut cxx_qt_lib::QQmlEngine>) {
    ffi::stitchcraft_prepare_window(engine);
}

/// How much of the window is shadow, for the Wayland compositor.
pub fn set_frame_margin(margin: i32) {
    ffi::stitchcraft_set_window_frame_margin(margin);
}

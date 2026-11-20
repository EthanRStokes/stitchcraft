#pragma once

#include <QtQml/QQmlEngine>

// Gets the engine ready for the frameless window: transparent surfaces (the
// shadow is drawn inside the window on Linux) and QWindowKit's QML types.
// Call before the window is created.
void stitchcraft_prepare_window(QQmlEngine &engine);

// Tells the compositor how much of each frameless top-level window is
// transparent shadow, so moves, snapping and screen clamping use the visible
// frame. Only does anything on Wayland.
void stitchcraft_set_window_frame_margin(int margin);

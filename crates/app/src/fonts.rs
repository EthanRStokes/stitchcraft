//! The bundled typeface. See `fonts.cpp`.

#[cxx::bridge]
mod ffi {
    unsafe extern "C++" {
        include!("fonts.h");
        fn stitchcraft_use_bundled_font();
    }
}

/// Makes Atkinson Hyperlegible Next the application font.
pub fn use_bundled_font() {
    ffi::stitchcraft_use_bundled_font();
}

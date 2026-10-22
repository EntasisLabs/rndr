//! Faces shipped with the desktop runner. Inter is the UI default. Arabic and
//! color emoji are bundled so a machine with an empty fontconfig still shapes
//! those scripts. Emoji is the CBDT/CBLC Noto Color Emoji build: swash paints
//! color bitmaps, and the COLRv1 file does not. CJK comes from the system
//! fallback list when the OS has it.

pub(super) const INTER_REGULAR: &[u8] = include_bytes!("../../fonts/Inter-Regular.ttf");
pub(super) const INTER_BOLD: &[u8] = include_bytes!("../../fonts/Inter-Bold.ttf");
pub(super) const INTER_ITALIC: &[u8] = include_bytes!("../../fonts/Inter-Italic.ttf");
pub(super) const NOTO_ARABIC: &[u8] = include_bytes!("../../fonts/NotoSansArabic-Regular.ttf");
pub(super) const NOTO_EMOJI: &[u8] = include_bytes!("../../fonts/NotoColorEmoji-CBDT.ttf");

pub(super) const BUNDLED: &[&[u8]] = &[
    INTER_REGULAR,
    INTER_BOLD,
    INTER_ITALIC,
    NOTO_ARABIC,
    NOTO_EMOJI,
];

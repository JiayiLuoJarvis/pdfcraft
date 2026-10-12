//! Public field values and fill settings.
//!
//! Text is drawn in PDF user space. The origin is the bottom-left of the page
//! and y increases upward.

/// Starting font size in points.
pub const DEFAULT_FONT_SIZE: f32 = 12.0;

/// Smallest font size the fitter tries, in points.
pub const MIN_FONT_SIZE: f32 = 7.0;

/// How far the fitter drops the font size on each attempt, in points.
pub const FONT_SIZE_STEP: f32 = 0.5;

/// Line height as a multiple of the font size.
pub const LINE_HEIGHT_FACTOR: f32 = 1.2;

/// Left and right padding inside a widget rectangle, in points.
pub const PAD_X: f32 = 1.0;

/// Bottom padding inside a widget rectangle, in points.
pub const PAD_BOTTOM: f32 = 1.0;

/// Top padding inside a widget rectangle, in points.
pub const PAD_TOP: f32 = 0.5;

/// Kind of `AcroForm` widget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FieldType {
    /// `/Tx`, or a widget with no `/FT`.
    Text,
    /// `/Btn` that is not a radio button or a push button.
    CheckBox,
    /// `/Ch` without the combo bit.
    ListBox,
    /// `/Ch` with the combo bit.
    ComboBox,
    /// `/Sig`.
    Signature,
    /// `/Btn` with the radio bit.
    RadioButton,
    /// `/Btn` with the push-button bit.
    PushButton,
}

/// One widget from [`crate::list_fields`].
///
/// Two widgets that share a name produce two values. The rectangle and the
/// checkbox on-state stay inside the crate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldInfo {
    /// `/T`, inherited from the first parent that has the key.
    pub name: String,
    /// Widget kind.
    pub field_type: FieldType,
    /// Zero-based page index.
    pub page_index: usize,
    /// Legal choice strings for a list box or combo box.
    ///
    /// Each `/Opt` export value and display value is included once.
    /// Other widget kinds leave this `None`.
    pub options: Option<Vec<String>>,
}

/// A value to draw into widgets that share a name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldValue {
    /// Text, list box, or combo box contents.
    ///
    /// A choice string must be one of the widget's export or display values.
    /// The library draws this string. It does not replace it with the display label.
    Text(String),
    /// Checkbox mark. `true` draws a check. `false` draws nothing.
    CheckBox(bool),
}

/// TrueType or TrueType-collection bytes for one fill call.
///
/// There is no process-wide default font.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FontBytes<'a> {
    /// Font file bytes.
    pub bytes: &'a [u8],
    /// Face index inside a collection. A single TrueType font uses `0`.
    pub index: u32,
}

/// What to do with a name in `fields` that matches no widget.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MissingName {
    /// Fail with [`crate::ErrorKind::UnknownField`].
    #[default]
    Error,
    /// Skip that name and fill the rest.
    Ignore,
}

/// Settings for one fill call.
///
/// Text is left aligned and bottom aligned. Per-field alignment, `line_below`,
/// and `absolute_lines` are not part of this version.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FillOptions<'a> {
    /// Font used when a text or choice string is drawn.
    ///
    /// Checkbox marks do not use this font. An empty string does not use it either.
    pub font: Option<FontBytes<'a>>,
    /// Starting size in points. The fitter may step down to [`MIN_FONT_SIZE`].
    pub font_size: f32,
    /// Behavior for names that are not in the PDF.
    pub missing: MissingName,
}

impl Default for FillOptions<'_> {
    fn default() -> Self {
        Self {
            font: None,
            font_size: DEFAULT_FONT_SIZE,
            missing: MissingName::Error,
        }
    }
}

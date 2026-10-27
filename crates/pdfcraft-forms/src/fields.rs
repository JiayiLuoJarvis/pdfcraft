//! Fill inputs: field snapshots, values, options, and placement.

use std::collections::HashMap;

/// Starting font size in points.
pub(crate) const DEFAULT_FONT_SIZE: f32 = 12.0;

/// Line height as a multiple of the font size.
pub(crate) const LINE_HEIGHT_FACTOR: f32 = 1.2;

/// Left and right padding inside a widget rectangle, in points.
pub(crate) const PAD_X: f32 = 1.0;

/// Bottom padding inside a widget rectangle, in points.
pub(crate) const PAD_BOTTOM: f32 = 1.0;

/// Top padding inside a widget rectangle, in points.
pub(crate) const PAD_TOP: f32 = 0.5;

/// Kind of `AcroForm` widget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
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
/// Two widgets that share a name produce two values. The rectangle stays inside the crate.
///
/// The library constructs this value and returns it from
/// [`crate::list_fields`]. External code cannot build it with a struct
/// literal. In patterns, use `..` (for example,
/// `FieldInfo { name, .. }`) so added fields do not break the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
#[non_exhaustive]
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
///
/// Match with a wildcard arm so added value kinds do not break the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
#[non_exhaustive]
pub enum FieldValue {
    /// Text, list box, or combo box contents.
    ///
    /// A choice string must be one of the widget's export or display values.
    /// The library draws this string. It does not replace it with the display label.
    Text(String),
    /// Checkbox mark: `true` means checked and draws the mark, `false` means
    /// unchecked and draws nothing.
    CheckBox(bool),
}

/// Bytes of one TrueType font with a `glyf` table.
///
/// There is no process-wide default font. `index` selects a face. A collection
/// is reduced to that face before it is embedded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub struct FontBytes<'a> {
    /// Font file bytes.
    pub bytes: &'a [u8],
    /// Face index. A single TrueType font uses `0`.
    pub index: u32,
}

/// What to do with a name in `fields` that matches no widget.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[must_use]
pub enum MissingName {
    /// Fail with [`crate::ErrorKind::UnknownField`].
    #[default]
    Error,
    /// Skip that name and fill the rest.
    Ignore,
}

/// Horizontal placement of a text line inside a widget rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[must_use]
pub enum HAlign {
    /// Line starts at the left padding.
    #[default]
    Left,
    /// Line is centered in the full widget width.
    Center,
    /// Line ends at the right padding.
    Right,
}

/// Vertical placement of the text block inside a widget rectangle.
///
/// Placement uses each line's glyph ink `yMin` and `yMax`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[must_use]
pub enum VAlign {
    /// Block sits on the bottom padding.
    #[default]
    Bottom,
    /// Block is centered between the top and bottom padding.
    Center,
    /// Block hangs from the top padding.
    Top,
}

/// A stroke in PDF user space. The origin is the page bottom-left.
///
/// Defaults match the production decorative line: red, 0.8 pt.
///
/// [`Default`] is the same stroke as [`Self::new`] at the origin, so callers
/// can write `AbsoluteLine { x1, y1, ..Default::default() }`.
#[derive(Debug, Clone, Copy, PartialEq)]
#[must_use]
pub struct AbsoluteLine {
    /// Start x, in points.
    pub x1: f32,
    /// Start y, in points.
    pub y1: f32,
    /// End x, in points.
    pub x2: f32,
    /// End y, in points.
    pub y2: f32,
    /// Zero-based page index.
    pub page_index: usize,
    /// Stroke width in points.
    pub stroke_width: f32,
    /// Red channel in `0.0..=1.0`.
    pub red: f32,
    /// Green channel in `0.0..=1.0`.
    pub green: f32,
    /// Blue channel in `0.0..=1.0`.
    pub blue: f32,
}

impl AbsoluteLine {
    /// A red 0.8 pt stroke on page 0.
    pub fn new(x1: f32, y1: f32, x2: f32, y2: f32) -> Self {
        Self {
            x1,
            y1,
            x2,
            y2,
            page_index: 0,
            stroke_width: 0.8,
            red: 1.0,
            green: 0.0,
            blue: 0.0,
        }
    }
}

/// Settings for one [`crate::fill_and_flatten`] call.
///
/// Default text alignment is left and bottom.
///
/// The struct keeps functional record update working: `FillOptions { font_size,
/// ..FillOptions::default() }`. It stays constructible, so it is not
/// `#[non_exhaustive]`.
#[derive(Debug, Clone, PartialEq)]
#[must_use]
pub struct FillOptions<'a> {
    /// Font used when a text or choice string is drawn.
    ///
    /// Checkbox marks do not use this font. An empty string does not use it either.
    pub font: Option<FontBytes<'a>>,
    /// Starting size in points. The fitter may step down to 7 pt.
    pub font_size: f32,
    /// Behavior for names that are not in the PDF.
    ///
    /// Also applies to names in [`Self::h_align_overrides`] and
    /// [`Self::v_align_overrides`].
    pub missing: MissingName,
    /// Default horizontal alignment for text and choice fields.
    pub h_align: HAlign,
    /// Default vertical alignment for text and choice fields.
    pub v_align: VAlign,
    /// Per-field horizontal alignment. Unknown names follow [`Self::missing`].
    pub h_align_overrides: Option<&'a HashMap<String, HAlign>>,
    /// Per-field vertical alignment. Unknown names follow [`Self::missing`].
    pub v_align_overrides: Option<&'a HashMap<String, VAlign>>,
    /// Starting size per field name, in points. Unknown names follow [`Self::missing`].
    pub font_size_overrides: Option<&'a HashMap<String, f32>>,
    /// Field name to a point offset below the widget bottom. Draws a 1 pt black rule
    /// the width of the widget. The line does not need a font.
    pub line_below: Option<&'a HashMap<String, f32>>,
    /// Page-absolute strokes. Names here are not field names.
    pub absolute_lines: &'a [AbsoluteLine],
}

impl Default for AbsoluteLine {
    fn default() -> Self {
        Self::new(0.0, 0.0, 0.0, 0.0)
    }
}

impl Default for FillOptions<'_> {
    fn default() -> Self {
        Self {
            font: None,
            font_size: DEFAULT_FONT_SIZE,
            missing: MissingName::Error,
            h_align: HAlign::Left,
            v_align: VAlign::Bottom,
            h_align_overrides: None,
            v_align_overrides: None,
            font_size_overrides: None,
            line_below: None,
            absolute_lines: &[],
        }
    }
}

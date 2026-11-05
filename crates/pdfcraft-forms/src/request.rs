//! Fill inputs: field snapshots, values, and placement.

use std::collections::BTreeMap;

/// Kind of `AcroForm` widget.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
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

/// One widget from [`Form::fields`](crate::Form::fields).
///
/// Two widgets that share a name produce two values. The rectangle stays inside the crate.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub struct FieldInfo {
    name: String,
    field_type: FieldType,
    page_index: usize,
    options: Option<Vec<String>>,
}

impl FieldInfo {
    pub(crate) fn new(
        name: String,
        field_type: FieldType,
        page_index: usize,
        options: Option<Vec<String>>,
    ) -> Self {
        Self {
            name,
            field_type,
            page_index,
            options,
        }
    }

    /// `/T`, inherited from the first parent that has the key.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Widget kind.
    pub fn field_type(&self) -> FieldType {
        self.field_type
    }

    /// Zero-based page index.
    #[must_use]
    pub fn page_index(&self) -> usize {
        self.page_index
    }

    /// Legal choice strings for a list box or combo box.
    ///
    /// Each `/Opt` export value and display value is included once.
    /// Other widget kinds leave this [`None`].
    #[must_use]
    pub fn options(&self) -> Option<&[String]> {
        self.options.as_deref()
    }
}

/// A value to draw into widgets that share a name.
///
/// [`Self::CheckBox`]`(true)` draws the check. [`Self::CheckBox`]`(false)` draws nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub enum FieldValue {
    /// Text, list box, or combo box contents.
    ///
    /// A choice string must be one of the widget's export or display values.
    /// The library draws this string. It does not replace it with the display label.
    Text(String),
    /// Checkbox mark.
    ///
    /// `true` draws the check. `false` draws nothing.
    CheckBox(bool),
}

/// Document-wide text placement.
///
/// The fitter may lower [`Self::font_size`] to 7 pt in steps of 0.5 pt.
#[derive(Debug, Clone, Copy, PartialEq)]
#[must_use]
pub struct FillStyle {
    /// Starting font size in points.
    pub font_size: f32,
    /// Default horizontal alignment.
    pub h_align: HAlign,
    /// Default vertical alignment.
    pub v_align: VAlign,
}

impl Default for FillStyle {
    fn default() -> Self {
        Self {
            font_size: 12.0,
            h_align: HAlign::Left,
            v_align: VAlign::Bottom,
        }
    }
}

/// Per-field overrides of [`FillStyle`].
///
/// `None` keeps the document style. It does not reset that slot to left alignment or 12 pt.
///
/// A style on a checkbox is still checked. [`Self::font_size`] follows the same
/// rules as [`FillStyle::font_size`]. The size and the alignments do not change
/// the check mark.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[must_use]
pub struct FieldStyle {
    /// Starting size in points, when set.
    pub font_size: Option<f32>,
    /// Horizontal alignment, when set.
    pub h_align: Option<HAlign>,
    /// Vertical alignment, when set.
    pub v_align: Option<VAlign>,
}

/// One named value to draw.
///
/// The name is the key in [`FillRequest::fields`].
#[derive(Debug, Clone, PartialEq)]
#[must_use]
pub struct FieldFill {
    /// Value drawn into every widget with this name.
    pub value: FieldValue,
    /// Overrides for this name.
    pub style: FieldStyle,
}

impl FieldFill {
    /// A text value with [`FieldStyle::default`].
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            value: FieldValue::Text(text.into()),
            style: FieldStyle::default(),
        }
    }

    /// A checkbox value with [`FieldStyle::default`].
    ///
    /// `true` draws the check. `false` draws nothing.
    pub fn check(checked: bool) -> Self {
        Self {
            value: FieldValue::CheckBox(checked),
            style: FieldStyle::default(),
        }
    }
}

/// A 1 pt black rule, as wide as the widget, drawn below its bottom edge.
///
/// The name is the key in [`FillRequest::rules`]. The rule does not need a
/// font, and it does not need a [`FieldFill`] for the same name.
#[derive(Debug, Clone, Copy, PartialEq)]
#[must_use]
pub struct FieldRule {
    /// Offset from the widget bottom, in points. A finite negative value is legal.
    pub offset: f32,
}

impl FieldRule {
    /// A rule `offset` points below the widget bottom.
    pub fn new(offset: f32) -> Self {
        Self { offset }
    }
}

/// Width and color of an [`AbsoluteLine`].
///
/// [`Default`] is red, 0.8 pt, matching [`AbsoluteLine::new`].
/// Channels are written as given. This crate only rejects non-finite numbers.
#[derive(Debug, Clone, Copy, PartialEq)]
#[must_use]
pub struct StrokeStyle {
    /// Stroke width in points.
    pub width: f32,
    /// Red channel.
    pub red: f32,
    /// Green channel.
    pub green: f32,
    /// Blue channel.
    pub blue: f32,
}

impl Default for StrokeStyle {
    fn default() -> Self {
        Self {
            width: 0.8,
            red: 1.0,
            green: 0.0,
            blue: 0.0,
        }
    }
}

/// A stroke in PDF user space. The origin is the page bottom-left.
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
    /// Stroke width and color.
    pub style: StrokeStyle,
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
            style: StrokeStyle::default(),
        }
    }
}

impl Default for AbsoluteLine {
    fn default() -> Self {
        Self::new(0.0, 0.0, 0.0, 0.0)
    }
}

/// What to do with a name in [`FillRequest`] that matches no widget.
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

/// Values, rules, and strokes for one [`crate::Fill`].
///
/// [`Self::fields`] and [`Self::rules`] are [`BTreeMap`]s, so names are visited
/// in byte order. A name may appear in both maps.
#[derive(Debug, Clone, PartialEq)]
#[must_use]
pub struct FillRequest {
    /// Document-wide text placement.
    pub style: FillStyle,
    /// Behavior for names that are not in the PDF.
    pub missing: MissingName,
    /// Field name to the value to draw. Keys compare as bytes.
    pub fields: BTreeMap<String, FieldFill>,
    /// Field name to a rule below that widget. Keys may overlap [`Self::fields`].
    pub rules: BTreeMap<String, FieldRule>,
    /// Page-absolute strokes, in the caller's order.
    pub lines: Vec<AbsoluteLine>,
}

impl FillRequest {
    /// A request with [`FillStyle::default`], [`MissingName::Error`], and no rules or lines.
    pub fn new(fields: BTreeMap<String, FieldFill>) -> Self {
        Self {
            style: FillStyle::default(),
            missing: MissingName::Error,
            fields,
            rules: BTreeMap::new(),
            lines: Vec::new(),
        }
    }
}

impl Default for FillRequest {
    fn default() -> Self {
        Self::new(BTreeMap::new())
    }
}

#![allow(clippy::wildcard_imports)]
use super::*;

/// Text and paragraph-level settings for one paragraph.
#[derive(Debug)]
pub struct Paragraph {
    pub(crate) items: Vec<InlineItem>,
    pub(crate) first_line_indent: Length<NonNegative>,
    pub(crate) align: TextAlign,
    pub(crate) color: Option<Color>,
    pub(crate) font_size: Option<Length<Positive>>,
    pub(crate) active_link: Option<LinkTarget>,
    pub(crate) error: Option<Error>,
}

impl Default for Paragraph {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            first_line_indent: NonNegative::ZERO,
            align: TextAlign::Start,
            color: None,
            font_size: None,
            active_link: None,
            error: None,
        }
    }
}

impl Paragraph {
    /// Sets this paragraph's text color. Overrides the parent flow color.
    pub fn color(&mut self, color: Color) {
        self.color = Some(color);
    }

    /// Sets this paragraph's font size. Overrides the parent flow size.
    pub fn font_size(&mut self, size: impl IntoPositive) {
        match size.into_positive() {
            Ok(size) => self.font_size = Some(size),
            Err(error) => self.note(error),
        }
    }

    /// Appends `s` to the paragraph.
    pub fn text(&mut self, s: impl AsRef<str>) {
        self.append(Face::Body, s.as_ref());
    }

    /// Appends `s` using the face registered by [`Document::bold_font`].
    pub fn bold(&mut self, s: impl AsRef<str>) {
        self.append(Face::Bold, s.as_ref());
    }

    /// Appends a fixed-width blank. Stroke settings keep their defaults when the result is ignored.
    pub fn blank(&mut self, width: impl IntoNonNegative) -> Blank<'_> {
        match width.into_non_negative() {
            Ok(width) => self.push_blank(BlankWidth::Fixed(width)),
            Err(error) => {
                self.note(error);
                Blank {
                    paragraph: self,
                    index: usize::MAX,
                }
            }
        }
    }

    /// Appends a blank `ems` times the paragraph font size wide.
    pub fn blank_em(&mut self, ems: u16) -> Blank<'_> {
        self.push_blank(BlankWidth::Em(ems))
    }

    /// Appends a blank that fills the rest of its visual line.
    ///
    /// A later non-empty run fails assembly with [`Error::RunAfterFillBlank`].
    pub fn blank_to_end(&mut self) -> Blank<'_> {
        self.push_blank(BlankWidth::ToEndOfLine)
    }

    /// Sets the horizontal inset of the first visual line.
    pub fn first_line_indent(&mut self, indent: impl IntoNonNegative) {
        match indent.into_non_negative() {
            Ok(indent) => self.first_line_indent = indent,
            Err(error) => self.note(error),
        }
    }

    /// Centers each visual line in the current frame.
    pub fn center(&mut self) {
        self.align = TextAlign::Center;
    }

    /// Places each visual line at the end of the current frame.
    pub fn end(&mut self) {
        self.align = TextAlign::End;
    }

    /// Appends the text and bold calls inside `f` as one link to `uri`.
    ///
    /// `f` returns `()` because it runs inside [`Flow::paragraph`], which cannot
    /// use `?`. An empty `uri` is stored as [`Error::EmptyLink`] and `f` does not run.
    pub fn link(&mut self, uri: impl AsRef<str>, f: impl FnOnce(&mut Paragraph)) {
        let target = match LinkTarget::uri(uri.as_ref()) {
            Ok(target) => target,
            Err(error) => {
                self.note(error);
                return;
            }
        };
        let previous = self.active_link.replace(target);
        f(self);
        self.active_link = previous;
    }

    fn append(&mut self, face: Face, text: &str) {
        if text.is_empty() {
            return;
        }
        if self.sealed() {
            self.note(Error::RunAfterFillBlank);
            return;
        }
        if let Some(InlineItem::Text(last)) = self.items.last_mut() {
            if last.face == face && last.link == self.active_link {
                last.text.push_str(text);
                return;
            }
        }
        self.items.push(InlineItem::Text(
            TextRun::new(text, face).with_link(self.active_link.clone()),
        ));
    }

    fn push_blank(&mut self, width: BlankWidth) -> Blank<'_> {
        if self.sealed() {
            self.note(Error::RunAfterFillBlank);
            return Blank {
                paragraph: self,
                index: usize::MAX,
            };
        }
        self.items.push(InlineItem::Blank(InlineBlank::new(width)));
        let index = self.items.len() - 1;
        Blank {
            paragraph: self,
            index,
        }
    }

    fn sealed(&self) -> bool {
        matches!(
            self.items.last(),
            Some(InlineItem::Blank(blank)) if matches!(blank.width, BlankWidth::ToEndOfLine)
        )
    }

    pub(crate) fn note(&mut self, error: Error) {
        if self.error.is_none() {
            self.error = Some(error);
        }
    }
}

/// Stroke settings for the blank just appended to a [`Paragraph`].
pub struct Blank<'a> {
    pub(crate) paragraph: &'a mut Paragraph,
    pub(crate) index: usize,
}

impl Blank<'_> {
    /// Sets the stroke thickness. The default is 1 pt.
    pub fn thickness(&mut self, thickness: impl IntoPositive) -> &mut Self {
        match thickness.into_positive() {
            Ok(thickness) => {
                if let Some(blank) = self.blank_mut() {
                    blank.thickness = thickness;
                }
            }
            Err(error) => self.paragraph.note(error),
        }
        self
    }

    /// Sets the stroke color. The default is the paragraph text color.
    pub fn color(&mut self, color: Color) -> &mut Self {
        if let Some(blank) = self.blank_mut() {
            blank.color = Some(color);
        }
        self
    }

    /// Sets the distance from the baseline down to the stroke. The default is 1 pt.
    pub fn baseline_offset(&mut self, offset: impl IntoNonNegative) -> &mut Self {
        match offset.into_non_negative() {
            Ok(offset) => {
                if let Some(blank) = self.blank_mut() {
                    blank.baseline_offset = offset;
                }
            }
            Err(error) => self.paragraph.note(error),
        }
        self
    }

    fn blank_mut(&mut self) -> Option<&mut InlineBlank> {
        match self.paragraph.items.get_mut(self.index) {
            Some(InlineItem::Blank(blank)) => Some(blank),
            _ => None,
        }
    }
}

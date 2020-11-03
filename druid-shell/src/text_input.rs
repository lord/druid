use std::ops::Range;
use std::borrow::Cow;
use crate::common_util::Counter;
use crate::window::WinHandler;
use crate::keyboard::{KbKey, KeyEvent};
use crate::kurbo::{Rect, Point};
use crate::piet::HitTestPoint;

/// A token that uniquely identifies a text input field inside a window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Hash)]
/// TODO
pub struct TextInputToken(u64);

impl TextInputToken {
    /// A token that does not correspond to any text input.
    pub const INVALID: TextInputToken = TextInputToken(0);

    /// Create a new token.
    pub fn next() -> TextInputToken {
        static TEXT_FIELD_COUNTER: Counter = Counter::new();
        TextInputToken(TEXT_FIELD_COUNTER.next())
    }

    /// Create a new token from a raw value.
    pub const fn from_raw(id: u64) -> TextInputToken {
        TextInputToken(id)
    }

    /// Get the raw value for a token.
    pub const fn into_raw(self) -> u64 {
        self.0
    }
}

/// All ranges, lengths, and indices are specified in UTF-8 code units, unless specified otherwise.
pub trait TextInputHandler {
    /// Gets the range of the document that is currently selected.
    /// If the selection is a vertical caret bar, then `range.start == range.end`.
    /// Both `range.start` and `range.end` must be less than or equal to the value returned
    /// from `TextInputHandler::len()`.
    fn selected_range(&mut self) -> Range<usize>;

    /// Sets the range of the document that is currently selected.
    /// If the selection is a vertical caret bar, then `range.start == range.end`.
    /// Both `range.start` and `range.end` must be less than or equal to the value returned
    /// from `TextInputHandler::len()`.
    fn set_selected_range(&mut self, range: Range<usize>);

    /// Gets the range of the document that is the input method's composition region.
    /// Both `range.start` and `range.end` must be less than or equal to the value returned
    /// from `TextInputHandler::len()`.
    fn composition_range(&mut self) -> Option<Range<usize>>;

    /// Sets the range of the document that is the input method's composition region.
    /// Both `range.start` and `range.end` must be less than or equal to the value returned
    /// from `TextInputHandler::len()`.
    fn set_composition_range(&mut self, range: Option<Range<usize>>);

    /// Returns true if `i==0`, `i==TextInputHandler::len()`, or `i` is the first byte of a UTF-8 code point sequence.
    /// Returns false otherwise, including if `i>TextInputHandler::len()`.
    /// Equivalent in functionality to `String::is_char_boundary`.
    fn is_char_boundary(&mut self, i: usize) -> bool;

    /// Returns the length of the document in UTF-8 code units. Equivalent to `String::len`.
    fn len(&mut self) -> usize;

    /// Returns the contents of some range of the document.
    /// If `range.start` or `range.end` do not fall on a code point sequence boundary, this method may panic.
    fn slice<'a>(&'a mut self, range: Range<usize>) -> Cow<'a, str>;

    /// Converts the document into UTF-8, looks up the range specified by `utf8_range` (in UTF-8 code units), reencodes
    /// that substring into UTF-16, and then returns the number of UTF-16 code units in that substring.
    ///
    /// You can override this if you have some faster system to determine string length.
    fn utf8_to_utf16<'a>(&'a mut self, utf8_range: Range<usize>) -> usize {
        self.slice(utf8_range).encode_utf16().count()
    }

    /// Converts the document into UTF-16, looks up the range specified by `utf16_range` (in UTF-16 code units), reencodes
    /// that substring into UTF-8, and then returns the number of UTF-8 code units in that substring.
    ///
    /// You can override this if you have some faster system to determine string length.
    fn utf16_to_utf8<'a>(&'a mut self, utf16_range: Range<usize>) -> usize {
        if utf16_range.is_empty() {
            return 0;
        }
        let doc_range = 0..self.len();
        let text = self.slice(doc_range);
        let utf16: Vec<u16> = text.encode_utf16().skip(utf16_range.start).take(utf16_range.end).collect();
        String::from_utf16_lossy(&utf16).len()
    }

    /// Replaces a range of the text document with `text`.
    /// If `range.start` or `range.end` do not fall on a code point sequence boundary, this method may panic.
    /// Equivalent to `String::replace_range`.
    fn replace_range(&mut self, range: Range<usize>, text: &str);

    /// Given a `Point`, determine the corresponding text position.
    fn hit_test_point(&mut self, point: Point) -> HitTestPoint;

    /// Returns the character range of the line (soft- or hard-wrapped) containing the character
    /// specified by `char_index`.
    /// TODO affinity?
    fn line_range(&mut self, char_index: usize) -> Range<usize>;

    /// Returns the bounding box, in window coordinates, of the visible text document. For instance,
    /// a text box's bounding box would be the rectangle of the border surrounding it, even if the text box is empty.
    /// If the text document is completely offscreen, return `None`.
    fn bounding_box(&mut self) -> Option<Rect>;

    /// Returns the bounding box, in window coordinates, of the range of text specified by `range`.
    /// Ranges will always be equal to or a subrange of some line range returned by `TextInputHandler::line_range`.
    /// If a range spans multiple lines, `slice_bounding_box` may panic.
    fn slice_bounding_box(&mut self, range: Range<usize>) -> Option<Rect>;
}

#[allow(dead_code)]
/// TODO docs
pub fn simulate_text_input<H: WinHandler + ?Sized>(handler: &mut H, token: Option<TextInputToken>, event: KeyEvent) -> bool {
    if handler.key_down(event.clone()) {
        return true;
    }

    if event.mods.ctrl() || event.mods.meta() || event.mods.alt() {
        return false;
    }

    let c = match event.key {
        KbKey::Character(c) => c,
        _ => return false,
    };
    let token = match token {
        Some(v) => v,
        None => return false,
    };
    let mut input_handler = match handler.text_input(token, true) {
        Some(v) => v,
        None => return false,
    };
    let selection = input_handler.selected_range();
    input_handler.replace_range(selection, &c);
    true
}

use std::ops::Range;
use std::borrow::Cow;
use crate::common_util::Counter;
use crate::window::WinHandler;
use crate::keyboard::{KbKey, KeyEvent};

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

/// TODO
pub trait TextInputHandler {
    fn selected_range(&mut self) -> Range<usize>;
    fn composition_range(&mut self) -> Option<Range<usize>>;
    fn set_selected_range(&mut self, range: Range<usize>);
    fn set_composition_range(&mut self, range: Option<Range<usize>>);
    fn replace(&mut self, range: Range<usize>, text: &str);
    fn slice<'a>(&'a mut self, range: Range<usize>) -> Cow<'a, str>;
    fn floor_index(&mut self, i: usize) -> usize;
    fn ceil_index(&mut self, i: usize) -> usize;
    fn len(&mut self) -> usize;
    // fn index_from_point(&mut self, point: Point2<f32>, flags: IndexFromPointFlags)
    //     -> Option<usize>;
    // fn frame(&mut self) -> Box2<f32>;
    // fn slice_bounds(&mut self, range: Range<usize>) -> (Box2<f32>, usize);
}

/// TODO
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
    input_handler.replace(selection, &c);
    true
}

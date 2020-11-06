// Copyright 2019 The Druid Authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#![allow(non_snake_case)]

use std::ffi::c_void;
use std::ops::Range;
use std::os::raw::c_uchar;

use super::window::get_edit_lock_from_window;
use crate::kurbo::Point;
use crate::text_input::{
    Action, Direction, Movement, TextInputHandler, VerticalMovement, WritingDirection,
};
use cocoa::base::{id, nil, BOOL};
use cocoa::foundation::{NSArray, NSPoint, NSRect, NSSize, NSString, NSUInteger};
use cocoa::{appkit::NSWindow, foundation::NSNotFound};
use objc::runtime::{Object, Sel};
use objc::{class, msg_send, sel, sel_impl};

// thanks to winit for the custom NSRange code:
// https://github.com/rust-windowing/winit/pull/518/files#diff-61be96e960785f102cb20ad8464eafeb6edd4245ea40224b3c3206c72cd5bf56R12-R34
#[repr(C)]
pub struct NSRange {
    pub location: NSUInteger,
    pub length: NSUInteger,
}
impl NSRange {
    pub const NONE: NSRange = NSRange::new(NSNotFound as NSUInteger, 0);
    #[inline]
    pub const fn new(location: NSUInteger, length: NSUInteger) -> NSRange {
        NSRange { location, length }
    }
}
unsafe impl objc::Encode for NSRange {
    fn encode() -> objc::Encoding {
        let encoding = format!(
            // TODO: Verify that this is correct
            "{{NSRange={}{}}}",
            NSUInteger::encode().as_str(),
            NSUInteger::encode().as_str(),
        );
        unsafe { objc::Encoding::from_str(&encoding) }
    }
}

pub extern "C" fn has_marked_text(this: &mut Object, _: Sel) -> BOOL {
    get_edit_lock_from_window(this, false)
        .map(|mut edit_lock| edit_lock.composition_range().is_some())
        .unwrap_or(false)
        .into()
}

pub extern "C" fn marked_range(this: &mut Object, _: Sel) -> NSRange {
    get_edit_lock_from_window(this, false)
        .and_then(|mut edit_lock| {
            edit_lock
                .composition_range()
                .map(|range| encode_nsrange(&mut edit_lock, range))
        })
        .unwrap_or(NSRange::NONE)
}

pub extern "C" fn selected_range(this: &mut Object, _: Sel) -> NSRange {
    let mut edit_lock = match get_edit_lock_from_window(this, false) {
        Some(v) => v,
        None => return NSRange::NONE,
    };
    let range = edit_lock.selected_range();
    // TODO convert utf8 -> utf16
    encode_nsrange(&mut edit_lock, range)
}

pub extern "C" fn set_marked_text(
    this: &mut Object,
    _: Sel,
    text: id,
    selected_range: NSRange,
    replacement_range: NSRange,
) {
    // TODO add thanks to yvt
    let mut edit_lock = match get_edit_lock_from_window(this, false) {
        Some(v) => v,
        None => return,
    };
    let mut composition_range = edit_lock.composition_range().unwrap_or_else(|| {
        // no existing composition range? default to replacement range, interpreted in absolute coordinates
        // undocumented by apple, see
        // https://github.com/yvt/Stella2/blob/076fb6ee2294fcd1c56ed04dd2f4644bf456e947/tcw3/pal/src/macos/window.rs#L1144-L1146
        decode_nsrange(&mut edit_lock, &replacement_range, 0).unwrap_or_else(|| {
            // no replacement range either? apparently we default to the selection in this case
            edit_lock.selected_range()
        })
    });

    let replace_range_offset = edit_lock
        .composition_range()
        .map(|range| range.start)
        .unwrap_or(0);

    let replace_range = decode_nsrange(&mut edit_lock, &replacement_range, replace_range_offset)
        .unwrap_or_else(|| {
            // default replacement range is already-exsiting composition range
            // undocumented by apple, see
            // https://github.com/yvt/Stella2/blob/076fb6ee2294fcd1c56ed04dd2f4644bf456e947/tcw3/pal/src/macos/window.rs#L1124-L1125
            composition_range.clone()
        });

    let text_string = parse_attributed_string(&text);
    // TODO utf8 -> utf16
    edit_lock.replace_range(replace_range.clone(), text_string);

    // Update the composition range
    composition_range.end -= replace_range.len();
    composition_range.end += text_string.len();
    if composition_range.len() == 0 {
        edit_lock.set_composition_range(None);
    } else {
        edit_lock.set_composition_range(Some(composition_range));
    };

    // Update the selection
    if let Some(selection_range) =
        decode_nsrange(&mut edit_lock, &selected_range, replace_range.start)
    {
        edit_lock.set_selected_range(selection_range);
    }
}

pub extern "C" fn unmark_text(this: &mut Object, _: Sel) {
    let mut edit_lock = match get_edit_lock_from_window(this, false) {
        Some(v) => v,
        None => return,
    };
    edit_lock.set_composition_range(None);
}

pub extern "C" fn valid_attributes_for_marked_text(this: &mut Object, _: Sel) -> id {
    // we don't support any attributes
    unsafe { NSArray::array(nil) }
}

pub extern "C" fn attributed_substring_for_proposed_range(
    this: &mut Object,
    _: Sel,
    proposed_range: NSRange,
    actual_range: *mut c_void,
) -> id {
    let mut edit_lock = match get_edit_lock_from_window(this, false) {
        Some(v) => v,
        None => return nil,
    };
    let range = match decode_nsrange(&mut edit_lock, &proposed_range, 0) {
        Some(v) => v,
        None => return nil,
    };
    if !actual_range.is_null() {
        let ptr = actual_range as *mut NSRange;
        let range_utf16 = encode_nsrange(&mut edit_lock, range.clone());
        unsafe {
            *ptr = range_utf16;
        }
    }
    let text = edit_lock.slice(range);
    unsafe {
        let ns_string = NSString::alloc(nil).init_str(&text);
        let attr_string: id = msg_send![class!(NSAttributedString), alloc];
        msg_send![attr_string, initWithString: ns_string]
    }
}

pub extern "C" fn insert_text(this: &mut Object, _: Sel, text: id, replacement_range: NSRange) {
    let mut edit_lock = match get_edit_lock_from_window(this, true) {
        Some(v) => v,
        None => return,
    };
    let text_string = parse_attributed_string(&text);

    // yvt notes:
    // [The null range case] is undocumented, but it seems that it means
    // the whole marked text or selected text should be finalized
    // and replaced with the given string.
    // https://github.com/yvt/Stella2/blob/076fb6ee2294fcd1c56ed04dd2f4644bf456e947/tcw3/pal/src/macos/window.rs#L1041-L1043
    let converted_range = decode_nsrange(&mut edit_lock, &replacement_range, 0)
        .or_else(|| edit_lock.composition_range())
        .unwrap_or_else(|| edit_lock.selected_range());

    edit_lock.replace_range(converted_range.clone(), text_string);
    edit_lock.set_composition_range(None);
    // move the caret next to the inserted text
    let caret_index = converted_range.start + text_string.len();
    edit_lock.set_selected_range(caret_index..caret_index);
}

pub extern "C" fn character_index_for_point(
    this: &mut Object,
    _: Sel,
    point: NSPoint,
) -> NSUInteger {
    let mut edit_lock = match get_edit_lock_from_window(this, true) {
        Some(v) => v,
        None => return 0,
    };
    let hit_test = edit_lock.hit_test_point(Point::new(point.x, point.y));
    hit_test.idx as NSUInteger
}

pub extern "C" fn first_rect_for_character_range(
    this: &mut Object,
    _: Sel,
    character_range: NSRange,
    actual_range: *mut c_void,
) -> NSRect {
    let mut edit_lock = match get_edit_lock_from_window(this, true) {
        Some(v) => v,
        None => return NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(0.0, 0.0)),
    };
    let mut range = decode_nsrange(&mut edit_lock, &character_range, 0).unwrap_or(0..0);
    {
        let line_range = edit_lock.line_range(range.start);
        range.end = usize::min(range.end, line_range.end);
    }
    let rect = match edit_lock.slice_bounding_box(range.clone()) {
        Some(v) => v,
        None => return NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(0.0, 0.0)),
    };
    if !actual_range.is_null() {
        let ptr = actual_range as *mut NSRange;
        let range_utf16 = encode_nsrange(&mut edit_lock, range);
        unsafe {
            *ptr = range_utf16;
        }
    }
    let view_space_rect = NSRect::new(
        NSPoint::new(rect.x0, rect.y0),
        NSSize::new(rect.width(), rect.height()),
    );
    unsafe {
        let window_space_rect: NSRect =
            msg_send![this as *const _, convertRect: view_space_rect toView: nil];
        let window: id = msg_send![this as *const _, window];
        window.convertRectToScreen_(window_space_rect)
    }
}

pub extern "C" fn do_command_by_selector(_this: &mut Object, _: Sel, cmd: Sel) {
    let cmd = match cmd.name() {
        // see https://developer.apple.com/documentation/appkit/nsstandardkeybindingresponding?language=objc
        // and https://support.apple.com/en-us/HT201236
        // and https://support.apple.com/lv-lv/guide/mac-help/mh21243/mac
        "cancelOperation:" => None, // TODO
        "capitalizeWord:" => Some(Action::CapitalizeWord),
        "centerSelectionInVisibleArea:" => Some(Action::ScrollToSelection), // TODO
        "changeCaseOfLetter:" => Some(Action::SwapLetterCase),
        "complete:" => None, // TODO
        "deleteBackward:" => Some(Action::Delete(Movement::Grapheme(Direction::Upstream))),
        "deleteBackwardByDecomposingPreviousCharacter:" => Some(Action::DecomposingBackspace),
        "deleteForward:" => Some(Action::Delete(Movement::Grapheme(Direction::Downstream))),
        "deleteToBeginningOfLine:" => Some(Action::Delete(Movement::Line(Direction::Upstream))),
        "deleteToBeginningOfParagraph:" => Some(Action::Delete(Movement::ParagraphStart)),
        "deleteToEndOfLine:" => Some(Action::Delete(Movement::Line(Direction::Downstream))),
        "deleteToEndOfParagraph:" => Some(Action::Delete(Movement::ParagraphEnd)),
        "deleteToMark:" => None, // TODO
        "deleteWordBackward:" => Some(Action::Delete(Movement::Word(Direction::Upstream))),
        "deleteWordForward:" => Some(Action::Delete(Movement::Word(Direction::Downstream))),
        "indent:" => Some(Action::Indent),
        "insertBacktab:" => Some(Action::InsertBacktab),
        "insertContainerBreak:" => None,                  // TODO
        "insertDoubleQuoteIgnoringSubstitution:" => None, // TODO
        "insertLineBreak:" => Some(Action::InsertLineBreak),
        "insertNewline:" => Some(Action::InsertNewLine {
            ignore_autocomplete: false,
        }),
        "insertNewlineIgnoringFieldEditor:" => Some(Action::InsertNewLine {
            ignore_autocomplete: true,
        }),
        "insertParagraphSeparator:" => Some(Action::InsertParagraphBreak),
        "insertSingleQuoteIgnoringSubstitution:" => None,
        "insertTab:" => Some(Action::InsertTab {
            ignore_autocomplete: false,
        }),
        "insertTabIgnoringFieldEditor:" => Some(Action::InsertTab {
            ignore_autocomplete: true,
        }),
        "lowercaseWord:" => Some(Action::LowercaseWord),
        "makeBaseWritingDirectionLeftToRight:" => Some(Action::SetParagraphWritingDirection(
            WritingDirection::LeftToRight,
        )),
        "makeBaseWritingDirectionNatural:" => Some(Action::SetParagraphWritingDirection(
            WritingDirection::Natural,
        )),
        "makeBaseWritingDirectionRightToLeft:" => Some(Action::SetParagraphWritingDirection(
            WritingDirection::RightToLeft,
        )),
        "makeTextWritingDirectionLeftToRight:" => Some(Action::SetSelectionWritingDirection(
            WritingDirection::LeftToRight,
        )),
        "makeTextWritingDirectionNatural:" => Some(Action::SetSelectionWritingDirection(
            WritingDirection::Natural,
        )),
        "makeTextWritingDirectionRightToLeft:" => Some(Action::SetSelectionWritingDirection(
            WritingDirection::RightToLeft,
        )),
        "moveBackward:" => Some(Action::Move(Movement::Grapheme(Direction::Upstream))),
        "moveBackwardAndModifySelection:" => Some(Action::MoveSelecting(Movement::Grapheme(
            Direction::Upstream,
        ))),
        "moveDown:" => Some(Action::Move(Movement::Vertical(VerticalMovement::LineDown))),
        "moveDownAndModifySelection:" => Some(Action::MoveSelecting(Movement::Vertical(
            VerticalMovement::LineDown,
        ))),
        "moveForward:" => Some(Action::Move(Movement::Grapheme(Direction::Downstream))),
        "moveForwardAndModifySelection:" => Some(Action::MoveSelecting(Movement::Grapheme(
            Direction::Downstream,
        ))),
        "moveLeft:" => Some(Action::Move(Movement::Grapheme(Direction::Left))),
        "moveLeftAndModifySelection:" => {
            Some(Action::MoveSelecting(Movement::Grapheme(Direction::Left)))
        }
        "moveParagraphBackwardAndModifySelection:" => {
            Some(Action::MoveSelecting(Movement::ParagraphPrev))
        }
        "moveParagraphForwardAndModifySelection:" => {
            Some(Action::MoveSelecting(Movement::ParagraphNext))
        }
        "moveRight:" => Some(Action::Move(Movement::Grapheme(Direction::Right))),
        "moveRightAndModifySelection:" => {
            Some(Action::MoveSelecting(Movement::Grapheme(Direction::Right)))
        }
        "moveToBeginningOfDocument:" => Some(Action::Move(Movement::Vertical(
            VerticalMovement::DocumentStart,
        ))),
        "moveToBeginningOfDocumentAndModifySelection:" => Some(Action::MoveSelecting(
            Movement::Vertical(VerticalMovement::DocumentStart),
        )),
        "moveToBeginningOfLine:" => Some(Action::Move(Movement::Line(Direction::Upstream))),
        "moveToBeginningOfLineAndModifySelection:" => {
            Some(Action::MoveSelecting(Movement::Line(Direction::Upstream)))
        }
        "moveToBeginningOfParagraph:" => Some(Action::Move(Movement::ParagraphStart)),
        "moveToBeginningOfParagraphAndModifySelection:" => {
            Some(Action::MoveSelecting(Movement::ParagraphStart))
        }
        "moveToEndOfDocument:" => Some(Action::Move(Movement::Vertical(
            VerticalMovement::DocumentEnd,
        ))),
        "moveToEndOfDocumentAndModifySelection:" => Some(Action::MoveSelecting(
            Movement::Vertical(VerticalMovement::DocumentEnd),
        )),
        "moveToEndOfLine:" => Some(Action::Move(Movement::Line(Direction::Downstream))),
        "moveToEndOfLineAndModifySelection:" => {
            Some(Action::MoveSelecting(Movement::Line(Direction::Downstream)))
        }
        "moveToEndOfParagraph:" => Some(Action::Move(Movement::ParagraphEnd)),
        "moveToEndOfParagraphAndModifySelection:" => {
            Some(Action::MoveSelecting(Movement::ParagraphEnd))
        }
        "moveToLeftEndOfLine:" => Some(Action::Move(Movement::Line(Direction::Left))),
        "moveToLeftEndOfLineAndModifySelection:" => {
            Some(Action::MoveSelecting(Movement::Line(Direction::Left)))
        }
        "moveToRightEndOfLine:" => Some(Action::Move(Movement::Line(Direction::Right))),
        "moveToRightEndOfLineAndModifySelection:" => {
            Some(Action::MoveSelecting(Movement::Line(Direction::Right)))
        }
        "moveUp:" => Some(Action::Move(Movement::Vertical(VerticalMovement::LineUp))),
        "moveUpAndModifySelection:" => Some(Action::MoveSelecting(Movement::Vertical(
            VerticalMovement::LineUp,
        ))),
        "moveWordBackward:" => Some(Action::Move(Movement::Word(Direction::Upstream))),
        "moveWordBackwardAndModifySelection:" => {
            Some(Action::MoveSelecting(Movement::Word(Direction::Upstream)))
        }
        "moveWordForward:" => Some(Action::Move(Movement::Word(Direction::Downstream))),
        "moveWordForwardAndModifySelection:" => {
            Some(Action::MoveSelecting(Movement::Word(Direction::Downstream)))
        }
        "moveWordLeft:" => Some(Action::Move(Movement::Word(Direction::Left))),
        "moveWordLeftAndModifySelection:" => {
            Some(Action::MoveSelecting(Movement::Word(Direction::Left)))
        }
        "moveWordRight:" => Some(Action::Move(Movement::Word(Direction::Right))),
        "moveWordRightAndModifySelection:" => {
            Some(Action::MoveSelecting(Movement::Word(Direction::Right)))
        }
        "pageDown:" => Some(Action::Move(Movement::Vertical(VerticalMovement::PageDown))),
        "pageDownAndModifySelection:" => Some(Action::MoveSelecting(Movement::Vertical(
            VerticalMovement::PageDown,
        ))),
        "pageUp:" => Some(Action::Move(Movement::Vertical(VerticalMovement::PageUp))),
        "pageUpAndModifySelection:" => Some(Action::MoveSelecting(Movement::Vertical(
            VerticalMovement::PageUp,
        ))),
        "quickLookPreviewItems:" => None, // TODO
        "scrollLineDown:" => Some(Action::Scroll(VerticalMovement::LineDown)),
        "scrollLineUp:" => Some(Action::Scroll(VerticalMovement::LineUp)),
        "scrollPageDown:" => Some(Action::Scroll(VerticalMovement::PageDown)),
        "scrollPageUp:" => Some(Action::Scroll(VerticalMovement::PageUp)),
        "scrollToBeginningOfDocument:" => Some(Action::Scroll(VerticalMovement::DocumentStart)),
        "scrollToEndOfDocument:" => Some(Action::Scroll(VerticalMovement::DocumentEnd)),
        "selectAll:" => Some(Action::SelectAll),
        "selectLine:" => Some(Action::SelectLine),
        "selectParagraph:" => Some(Action::SelectParagraph),
        "selectToMark:" => None, // TODO
        "selectWord:" => Some(Action::SelectWord),
        "setMark:" => None,      // TODO
        "swapWithMark:" => None, // TODO
        "transpose:" => Some(Action::Transpose),
        "transposeWords:" => Some(Action::TransposeWord),
        "uppercaseWord:" => Some(Action::UppercaseWord),
        "yank:" => None, // TODO
        e => {
            eprintln!("unknown text editing command from macos: {}", e);
            None
        }
    };
    println!("{:?}", cmd);
}

/// Parses the UTF-16 `NSRange` into a UTF-8 `Range<usize>`.
/// `start_offset` is the UTF-8 offset into the document that `range` values are relative to. Set it to `0` if `range`
/// is absolute instead of relative.
/// Returns `None` if `range` was invalid; macOS often uses this to indicate some special null value.
fn decode_nsrange(
    edit_lock: &mut Box<dyn TextInputHandler>,
    range: &NSRange,
    start_offset: usize,
) -> Option<Range<usize>> {
    if range.location as usize >= i32::max_value() as usize {
        return None;
    }
    // TODO fix offsets if they don't lie on a unicode boundary, or if they're beyond the end of the document
    let start_offset_utf16 = edit_lock.utf8_to_utf16(0..start_offset);
    let location_utf16 = range.location as usize + start_offset_utf16;
    let length_utf16 = range.length as usize + start_offset_utf16;
    let start_utf8 = edit_lock.utf16_to_utf8(0..location_utf16);
    let end_utf8 =
        start_utf8 + edit_lock.utf16_to_utf8(location_utf16..location_utf16 + length_utf16);
    Some(start_utf8..end_utf8)
}

// Encodes the UTF-8 `Range<usize>` into a UTF-16 `NSRange`.
fn encode_nsrange(edit_lock: &mut Box<dyn TextInputHandler>, range: Range<usize>) -> NSRange {
    let start = edit_lock.utf8_to_utf16(0..range.start);
    let len = edit_lock.utf8_to_utf16(range);
    NSRange::new(start as NSUInteger, len as NSUInteger)
}

fn parse_attributed_string(text: &id) -> &str {
    unsafe {
        let nsstring = if msg_send![*text, isKindOfClass: class!(NSAttributedString)] {
            msg_send![*text, string]
        } else {
            // already a NSString
            *text
        };
        let slice =
            std::slice::from_raw_parts(nsstring.UTF8String() as *const c_uchar, nsstring.len());
        std::str::from_utf8_unchecked(slice)
    }
}

// Copyright 2018 The Druid Authors.
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

use std::any::Any;
use std::borrow::Cow;
use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

use druid_shell::kurbo::{Line, Size};
use druid_shell::piet::{
    Color, FontFamily, HitTestPoint, PietText, PietTextLayout, PietTextLayoutBuilder,
    RenderContext, Text, TextLayout, TextLayoutBuilder,
};

use druid_shell::{
    keyboard_types::Key, Application, Cursor, FileDialogOptions, FileDialogToken, FileInfo,
    FileSpec, HotKey, KeyEvent, Menu, MouseEvent, Region, SysMods, TextInputHandler,
    TextInputToken, TextInputUpdate, TimerToken, WinHandler, WindowBuilder, WindowHandle,
};

use druid_shell::kurbo::{Point, Rect};

const BG_COLOR: Color = Color::rgb8(0xff, 0xff, 0xff);
const COMPOSITION_BG_COLOR: Color = Color::rgb8(0xff, 0xd8, 0x6e);
const SELECTION_BG_COLOR: Color = Color::rgb8(0x87, 0xc5, 0xff);
const CARET_COLOR: Color = Color::rgb8(0x00, 0x82, 0xfc);
// const FG_COLOR: Color = Color::rgb8(0xf0, 0xf0, 0xea);
const FONT: FontFamily = FontFamily::SANS_SERIF;
const FONT_SIZE: f64 = 16.0;

#[derive(Default)]
struct AppState {
    size: Size,
    handle: WindowHandle,
    document: Rc<RefCell<DocumentState>>,
    text_input_token: Option<TextInputToken>,
}

#[derive(Default)]
struct DocumentState {
    text: String,
    selection: Range<usize>,
    composition: Option<Range<usize>>,
    text_engine: Option<PietText>,
    layout: Option<PietTextLayout>,
}

impl DocumentState {
    fn refresh_layout(&mut self) {
        let text_engine = self.text_engine.as_mut().unwrap();
        self.layout = Some(
            text_engine
                .new_text_layout(self.text.clone())
                .font(FONT, FONT_SIZE)
                .build()
                .unwrap(),
        );
    }
}

impl WinHandler for AppState {
    fn connect(&mut self, handle: &WindowHandle) {
        self.handle = handle.clone();
        let token = self.handle.add_text_input();
        self.handle.set_active_text_input(Some(token));
        self.text_input_token = Some(token);
        let mut doc = self.document.borrow_mut();
        doc.text_engine = Some(handle.text());
        doc.refresh_layout();
    }

    fn prepare_paint(&mut self) {
        self.handle.invalidate();
    }

    fn paint(&mut self, piet: &mut piet_common::Piet, _: &Region) {
        // TODO bidi
        let rect = self.size.to_rect();
        piet.fill(rect, &BG_COLOR);
        let doc = self.document.borrow();
        let layout = doc.layout.as_ref().unwrap();
        if let Some(composition_range) = doc.composition.as_ref() {
            let left_x = layout
                .hit_test_text_position(composition_range.start)
                .point
                .x;
            let right_x = layout.hit_test_text_position(composition_range.end).point.x;
            piet.fill(
                Rect::new(left_x, 0.0, right_x, FONT_SIZE),
                &COMPOSITION_BG_COLOR,
            );
        }
        if doc.selection.start != doc.selection.end {
            let left_x = layout.hit_test_text_position(doc.selection.start).point.x;
            let right_x = layout.hit_test_text_position(doc.selection.end).point.x;
            piet.fill(
                Rect::new(left_x, 0.0, right_x, FONT_SIZE),
                &SELECTION_BG_COLOR,
            );
        }
        piet.draw_text(layout, (0.0, 0.0));

        // draw caret
        let caret_x = layout.hit_test_text_position(doc.selection.end).point.x;
        piet.fill(
            Rect::new(caret_x - 1.0, 0.0, caret_x + 1.0, FONT_SIZE),
            &CARET_COLOR,
        );
    }

    fn command(&mut self, id: u32) {
        match id {
            0x100 => {
                self.handle.close();
                Application::global().quit()
            }
            _ => println!("unexpected id {}", id),
        }
    }

    fn key_down(&mut self, event: KeyEvent) -> bool {
        if event.key == Key::Character("c".to_string()) {
            // custom hotkey for pressing "c"
            println!("user pressed c! wow! setting selection to 0");

            // update internal selection state
            self.document.borrow_mut().selection = (0..0);

            // notify the OS that we've updated the selection
            self.handle.update_text_input(
                self.text_input_token.unwrap(),
                TextInputUpdate::SelectionChanged,
            );

            // repaint window
            self.handle.request_anim_frame();

            // return true prevents the keypress event from being handled as text input
            return true;
        }
        false
    }

    fn text_input(
        &mut self,
        _token: TextInputToken,
        _mutable: bool,
    ) -> Option<Box<dyn TextInputHandler>> {
        Some(Box::new(AppTextInputHandler {
            state: self.document.clone(),
            window_size: self.size.clone(),
            window_handle: self.handle.clone(),
        }))
    }

    fn size(&mut self, size: Size) {
        self.size = size;
    }

    fn request_close(&mut self) {
        self.handle.close();
    }

    fn destroy(&mut self) {
        Application::global().quit()
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}

struct AppTextInputHandler {
    // TODO real lock
    state: Rc<RefCell<DocumentState>>,
    window_size: Size,
    window_handle: WindowHandle,
}

impl TextInputHandler for AppTextInputHandler {
    fn selected_range(&mut self) -> Range<usize> {
        self.state.borrow().selection.clone()
    }
    fn composition_range(&mut self) -> Option<Range<usize>> {
        self.state.borrow().composition.clone()
    }
    fn set_selected_range(&mut self, range: Range<usize>) {
        self.state.borrow_mut().selection = range;
        self.window_handle.request_anim_frame();
    }
    fn set_composition_range(&mut self, range: Option<Range<usize>>) {
        self.state.borrow_mut().composition = range;
        self.window_handle.request_anim_frame();
    }
    fn replace_range(&mut self, range: Range<usize>, text: &str) {
        let mut doc = self.state.borrow_mut();
        doc.text.replace_range(range, text);
        doc.refresh_layout();
        self.window_handle.request_anim_frame();
    }
    fn slice<'a>(&'a mut self, range: Range<usize>) -> Cow<'a, str> {
        self.state.borrow().text[range].to_string().into()
    }
    fn is_char_boundary(&mut self, i: usize) -> bool {
        self.state.borrow().text.is_char_boundary(i)
    }
    fn len(&mut self) -> usize {
        self.state.borrow().text.len()
    }
    fn hit_test_point(&mut self, point: Point) -> HitTestPoint {
        self.state
            .borrow()
            .layout
            .as_ref()
            .unwrap()
            .hit_test_point(point)
    }
    fn bounding_box(&mut self) -> Option<Rect> {
        Some(Rect::new(
            0.0,
            0.0,
            self.window_size.width,
            self.window_size.height,
        ))
    }
    fn slice_bounding_box(&mut self, range: Range<usize>) -> Option<Rect> {
        let doc = self.state.borrow();
        let layout = doc.layout.as_ref().unwrap();
        let range_start_x = layout.hit_test_text_position(range.start).point.x;
        let range_end_x = layout.hit_test_text_position(range.end).point.x;
        Some(Rect::new(range_start_x, 0.0, range_end_x, FONT_SIZE))
    }
    fn line_range(&mut self, _char_index: usize) -> Range<usize> {
        // we don't have multiple lines, so no matter the input, output is the whole document
        0..self.state.borrow().text.len()
    }
}

fn main() {
    let app = Application::new().unwrap();
    let mut builder = WindowBuilder::new(app.clone());
    builder.set_handler(Box::new(AppState::default()));
    builder.set_title("IME example");
    let window = builder.build().unwrap();
    window.show();
    app.run(None);
}

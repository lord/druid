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
use std::ops::Range;
use std::borrow::Cow;
use std::cell::RefCell;
use std::rc::Rc;

use druid_shell::kurbo::{Line, Size};
use druid_shell::piet::{Color, RenderContext};

use druid_shell::{
    Application, Cursor, FileDialogOptions, FileDialogToken, FileInfo, FileSpec, HotKey, KeyEvent,
    Menu, MouseEvent, Region, SysMods, TimerToken, WinHandler, WindowBuilder, WindowHandle, TextInputToken, TextInputHandler, keyboard_types::Key,
};

use druid_shell::kurbo::{Rect, Point};

const BG_COLOR: Color = Color::rgb8(0x27, 0x28, 0x22);
const FG_COLOR: Color = Color::rgb8(0xf0, 0xf0, 0xea);
const CHAR_WIDTH: f64 = 10.0;
const CHAR_HEIGHT: f64 = 10.0;

#[derive(Default)]
struct AppState {
    size: Size,
    handle: WindowHandle,
    document: Rc<RefCell<DocumentState>>,
}

#[derive(Default, Debug)]
struct DocumentState {
    text: String,
    selection: Range<usize>,
    composition: Option<Range<usize>>,
}

impl WinHandler for AppState {
    fn connect(&mut self, handle: &WindowHandle) {
        self.handle = handle.clone();
        let token = self.handle.add_text_input();
        self.handle.set_active_text_input(Some(token));
    }

    fn prepare_paint(&mut self) {}

    fn paint(&mut self, piet: &mut piet_common::Piet, _: &Region) {
        let rect = self.size.to_rect();
        piet.fill(rect, &BG_COLOR);
        piet.stroke(Line::new((10.0, 50.0), (90.0, 90.0)), &FG_COLOR, 1.0);
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
            println!("user pressed c! wow!");
            // return true prevents the keypress event from being handled as text input
            return true;
        }
        false
    }

    fn text_input(&mut self, token: TextInputToken, mutable: bool) -> Option<Box<dyn TextInputHandler>> {
        Some(Box::new(AppTextInputHandler{
            state: self.document.clone(),
            window_size: self.size.clone(),
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
    }
    fn set_composition_range(&mut self, range: Option<Range<usize>>) {
        self.state.borrow_mut().composition = range;
    }
    fn replace(&mut self, range: Range<usize>, text: &str) {
        self.state.borrow_mut().text.replace_range(range, text);
    }
    fn slice<'a>(&'a mut self, range: Range<usize>) -> Cow<'a, str> {
        self.state.borrow().text[range].to_string().into()
    }
    fn floor_index(&mut self, mut i: usize) -> usize {
        let state = self.state.borrow();
        let text = &state.text;
        if i >= text.len() {
            return text.len();
        }
        while !text.is_char_boundary(i) {
            i -= 1;
        }
        i
    }
    fn ceil_index(&mut self, mut i: usize) -> usize {
        let state = self.state.borrow();
        let text = &state.text;
        if i >= text.len() {
            return text.len();
        }
        while !text.is_char_boundary(i) {
            i += 1;
        }
        i
    }
    fn len(&mut self) -> usize {
        self.state.borrow().text.len()
    }
    fn index_from_point(&mut self, point: Point, _flags: ()) -> Option<usize> {
        if point.x < 0.0 {
            None
        } else {
            Some((point.x / CHAR_WIDTH) as usize)
        }
    }
    fn frame(&mut self) -> Option<Rect> {
        Some(Rect::new(0.0, 0.0, self.window_size.width, self.window_size.height))
    }
    fn slice_bounds(&mut self, range: Range<usize>) -> Option<(Rect, usize)> {
        let rect = Rect::new(CHAR_WIDTH * range.start as f64, 0.0, CHAR_WIDTH * range.end as f64, CHAR_HEIGHT);
        println!("slice bounds rect: {:?}", &rect);
        Some((rect, range.end))
    }
}

impl Drop for AppTextInputHandler {
    fn drop(&mut self) {
        println!("new document state: {:?}", self.state.borrow())
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

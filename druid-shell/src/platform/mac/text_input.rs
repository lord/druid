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
use std::mem;
use std::ops::Range;
use std::sync::{Arc, Mutex, Weak};
use std::time::Instant;
use std::{any::Any, os::raw::c_uchar};

use block::ConcreteBlock;
use cocoa::base::{id, nil, BOOL, NO, YES};
use cocoa::foundation::{
    NSArray, NSAutoreleasePool, NSInteger, NSPoint, NSRect, NSSize, NSString, NSUInteger,
};
use cocoa::{
    appkit::{
        CGFloat, NSApp, NSApplication, NSAutoresizingMaskOptions, NSBackingStoreBuffered, NSEvent,
        NSView, NSViewHeightSizable, NSViewWidthSizable, NSWindow, NSWindowStyleMask,
    },
    foundation::NSNotFound,
};
use core_graphics::context::CGContextRef;
use foreign_types::ForeignTypeRef;
use lazy_static::lazy_static;
use log::{error, info};
use objc::declare::ClassDecl;
use objc::rc::WeakPtr;
use objc::runtime::{Class, Object, Protocol, Sel};
use objc::{class, msg_send, sel, sel_impl};

use crate::kurbo::{Point, Rect, Size, Vec2};
use crate::piet::{Piet, PietText, RenderContext};

use super::appkit::{
    NSRunLoopCommonModes, NSTrackingArea, NSTrackingAreaOptions, NSView as NSViewExt,
};
use super::application::Application;
use super::dialog;
use super::keyboard::{make_modifiers, KeyboardState};
use super::menu::Menu;
use super::util::{assert_main_thread, make_nsstring};
use crate::common_util::IdleCallback;
use crate::dialog::{FileDialogOptions, FileDialogType, FileInfo};
use crate::keyboard_types::KeyState;
use crate::mouse::{Cursor, CursorDesc, MouseButton, MouseButtons, MouseEvent};
use crate::region::Region;
use crate::scale::Scale;
use crate::text_input::{TextInputHandler, TextInputToken};
use crate::window::{FileDialogToken, IdleToken, TimerToken, WinHandler, WindowLevel, WindowState};
use crate::Error;

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


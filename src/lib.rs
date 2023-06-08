//! A platform integration to use [egui](https://github.com/emilk/egui) with [winit](https://github.com/rust-windowing/winit).
//!
//! You need to create a [`Platform`] and feed it with `winit::event::Event` events.
//! Use `begin_frame()` and `end_frame()` to start drawing the egui UI.
//! A basic usage example can be found [here](https://github.com/hasenbanck/egui_example).
#![warn(missing_docs)]

#[cfg(feature = "clipboard")]
use clipboard::{ClipboardContext, ClipboardProvider};
use egui::{
    math::{pos2, vec2},
    CtxRef,
};
use egui::{paint::ClippedShape, Key};
use winit::event::Event;
use winit::event::WindowEvent::*;
use winit::keyboard::{Key as WinKey, ModifiersState};

/// Configures the creation of the `Platform`.
pub struct PlatformDescriptor {
    /// Width of the window in physical pixel.
    pub physical_width: u32,
    /// Height of the window in physical pixel.
    pub physical_height: u32,
    /// HiDPI scale factor.
    pub scale_factor: f64,
    /// Egui font configuration.
    pub font_definitions: egui::FontDefinitions,
    /// Egui style configuration.
    pub style: egui::Style,
}

#[cfg(feature = "webbrowser")]
fn handle_links(output: &egui::Output) {
    if let Some(open_url) = &output.open_url {
        // This does not handle open_url.new_tab
        // webbrowser does not support web anyway
        if let Err(err) = webbrowser::open(&open_url.url) {
            eprintln!("Failed to open url: {}", err);
        }
    }
}

#[cfg(feature = "clipboard")]
fn handle_clipboard(output: &egui::Output, clipboard: Option<&mut ClipboardContext>) {
    if !output.copied_text.is_empty() {
        if let Some(clipboard) = clipboard {
            if let Err(err) = clipboard.set_contents(output.copied_text.clone()) {
                eprintln!("Copy/Cut error: {}", err);
            }
        }
    }
}

/// Provides the integration between egui and winit.
pub struct Platform {
    scale_factor: f64,
    context: CtxRef,
    raw_input: egui::RawInput,
    modifier_state: ModifiersState,
    pointer_pos: egui::Pos2,

    #[cfg(feature = "clipboard")]
    clipboard: Option<ClipboardContext>,
}

impl Platform {
    /// Creates a new `Platform`.
    pub fn new(descriptor: PlatformDescriptor) -> Self {
        let context = CtxRef::default();

        context.set_fonts(descriptor.font_definitions.clone());
        context.set_style(descriptor.style);
        let raw_input = egui::RawInput {
            pixels_per_point: Some(descriptor.scale_factor as f32),
            screen_rect: Some(egui::Rect::from_min_size(
                Default::default(),
                vec2(
                    descriptor.physical_width as f32,
                    descriptor.physical_height as f32,
                ) / descriptor.scale_factor as f32,
            )),
            ..Default::default()
        };

        Self {
            scale_factor: descriptor.scale_factor,
            context,
            raw_input,
            modifier_state: winit::keyboard::ModifiersState::empty(),
            pointer_pos: Default::default(),
            #[cfg(feature = "clipboard")]
            clipboard: ClipboardContext::new().ok(),
        }
    }

    /// Handles the given winit event and updates the egui context. Should be called before starting a new frame with `start_frame()`.
    pub fn handle_event<T>(&mut self, winit_event: &Event<T>) {
        match winit_event {
            Event::WindowEvent {
                window_id: _window_id,
                event,
            } => match event {
                Resized(physical_size) => {
                    self.raw_input.screen_rect = Some(egui::Rect::from_min_size(
                        Default::default(),
                        vec2(physical_size.width as f32, physical_size.height as f32)
                            / self.scale_factor as f32,
                    ));
                }
                ScaleFactorChanged {
                    scale_factor,
                    new_inner_size,
                } => {
                    self.scale_factor = *scale_factor;
                    self.raw_input.pixels_per_point = Some(*scale_factor as f32);
                    self.raw_input.screen_rect = Some(egui::Rect::from_min_size(
                        Default::default(),
                        vec2(new_inner_size.width as f32, new_inner_size.height as f32)
                            / self.scale_factor as f32,
                    ));
                }
                MouseInput { state, button, .. } => {
                    if let winit::event::MouseButton::Other(..) = button {
                    } else {
                        self.raw_input.events.push(egui::Event::PointerButton {
                            pos: self.pointer_pos,
                            button: match button {
                                winit::event::MouseButton::Left => egui::PointerButton::Primary,
                                winit::event::MouseButton::Right => egui::PointerButton::Secondary,
                                winit::event::MouseButton::Middle => egui::PointerButton::Middle,
                                winit::event::MouseButton::Other(_) => unreachable!(),
                            },
                            pressed: *state == winit::event::ElementState::Pressed,
                            modifiers: Default::default(),
                        });
                    }
                }
                MouseWheel { delta, .. } => {
                    match delta {
                        winit::event::MouseScrollDelta::LineDelta(x, y) => {
                            let line_height = 24.0; // TODO as in egui_glium
                            self.raw_input.scroll_delta = vec2(*x, *y) * line_height;
                        }
                        winit::event::MouseScrollDelta::PixelDelta(delta) => {
                            // Actually point delta
                            self.raw_input.scroll_delta = vec2(delta.x as f32, delta.y as f32);
                        }
                    }
                }
                CursorMoved { position, .. } => {
                    self.pointer_pos = pos2(
                        position.x as f32 / self.scale_factor as f32,
                        position.y as f32 / self.scale_factor as f32,
                    );
                    self.raw_input
                        .events
                        .push(egui::Event::PointerMoved(self.pointer_pos));
                }
                CursorLeft { .. } => {
                    self.raw_input.events.push(egui::Event::PointerGone);
                }
                ModifiersChanged(input) => self.modifier_state = input.state(),
                KeyboardInput { event, .. } => {
                    let pressed = event.state == winit::event::ElementState::Pressed;

                    if pressed {
                        let is_ctrl = self.modifier_state.control_key();
                        let is_char = |test_char| matches!(&event.logical_key, WinKey::Character(c) if c.eq_ignore_ascii_case(test_char));
                        if is_ctrl && is_char("c") {
                            self.raw_input.events.push(egui::Event::Copy)
                        } else if is_ctrl && is_char("x") {
                            self.raw_input.events.push(egui::Event::Cut)
                        } else if is_ctrl && is_char("v") {
                            #[cfg(feature = "clipboard")]
                            if let Some(ref mut clipboard) = self.clipboard {
                                if let Ok(contents) = clipboard.get_contents() {
                                    self.raw_input.events.push(egui::Event::Text(contents))
                                }
                            }
                        } else if let Some(key) = winit_to_egui_key_code(&event.logical_key) {
                            self.raw_input.events.push(egui::Event::Key {
                                key,
                                pressed: event.state == winit::event::ElementState::Pressed,
                                modifiers: winit_to_egui_modifiers(self.modifier_state),
                            });
                        }
                    }

                    if let Some(text) = &event.text {
                        if !self.modifier_state.control_key() && !self.modifier_state.super_key() {
                            let filtered = text
                                .chars()
                                .filter(|ch| is_printable(*ch))
                                .collect::<String>();
                            if !filtered.is_empty() {
                                self.raw_input.events.push(egui::Event::Text(filtered));
                            }
                        }
                    }
                }
                _ => {}
            },
            Event::DeviceEvent { .. } => {}
            _ => {}
        }
    }

    /// Returns `true` if egui should handle the event exclusively. Check this to
    /// avoid unexpected interactions, e.g. a mouse click registering "behind" the UI.
    pub fn captures_event<T>(&self, winit_event: &Event<T>) -> bool {
        match winit_event {
            Event::WindowEvent {
                window_id: _window_id,
                event,
            } => match event {
                KeyboardInput { .. } | ModifiersChanged(_) => self.context().wants_keyboard_input(),

                MouseWheel { .. } | MouseInput { .. } => self.context().wants_pointer_input(),

                CursorMoved { .. } => self.context().is_using_pointer(),

                _ => false,
            },

            _ => false,
        }
    }

    /// Updates the internal time for egui used for animations. `elapsed_seconds` should be the seconds since some point in time (for example application start).
    pub fn update_time(&mut self, elapsed_seconds: f64) {
        self.raw_input.time = Some(elapsed_seconds);
    }

    /// Starts a new frame by providing a new `Ui` instance to write into.
    pub fn begin_frame(&mut self) {
        self.context.begin_frame(self.raw_input.take());
    }

    /// Ends the frame. Returns what has happened as `Output` and gives you the draw instructions as `PaintJobs`.
    pub fn end_frame(&mut self) -> (egui::Output, Vec<ClippedShape>) {
        // otherwise the below line gets flagged by clippy if both clipboard and webbrowser features are disabled
        #[allow(clippy::let_and_return)]
        let parts = self.context.end_frame();

        #[cfg(feature = "clipboard")]
        handle_clipboard(&parts.0, self.clipboard.as_mut());

        #[cfg(feature = "webbrowser")]
        handle_links(&parts.0);

        parts
    }

    /// Returns the internal egui context.
    pub fn context(&self) -> CtxRef {
        self.context.clone()
    }
}

/// Translates winit to egui keycodes.
#[inline]
fn winit_to_egui_key_code(key: &WinKey) -> Option<egui::Key> {
    Some(match key {
        WinKey::Escape => Key::Escape,
        WinKey::Insert => Key::Insert,
        WinKey::Home => Key::Home,
        WinKey::Delete => Key::Delete,
        WinKey::End => Key::End,
        WinKey::PageDown => Key::PageDown,
        WinKey::PageUp => Key::PageUp,
        WinKey::ArrowLeft => Key::ArrowLeft,
        WinKey::ArrowUp => Key::ArrowUp,
        WinKey::ArrowRight => Key::ArrowRight,
        WinKey::ArrowDown => Key::ArrowDown,
        WinKey::Backspace => Key::Backspace,
        WinKey::Enter => Key::Enter,
        WinKey::Tab => Key::Tab,
        WinKey::Space => Key::Space,

        WinKey::Character(c) => match c.as_str() {
            "A" | "a" => Key::A,
            "K" | "k" => Key::K,
            "U" | "u" => Key::U,
            "W" | "w" => Key::W,
            "Z" | "z" => Key::Z,
            _ => {
                return None;
            }
        },

        _ => {
            return None;
        }
    })
}

/// Translates winit to egui modifier keys.
#[inline]
fn winit_to_egui_modifiers(modifiers: ModifiersState) -> egui::Modifiers {
    egui::Modifiers {
        alt: modifiers.alt_key(),
        ctrl: modifiers.control_key(),
        shift: modifiers.shift_key(),
        #[cfg(target_os = "macos")]
        mac_cmd: modifiers.super_key(),
        #[cfg(target_os = "macos")]
        command: modifiers.super_key(),
        #[cfg(not(target_os = "macos"))]
        mac_cmd: false,
        #[cfg(not(target_os = "macos"))]
        command: modifiers.control_key(),
    }
}

/// We only want printable characters and ignore all special keys.
#[inline]
fn is_printable(chr: char) -> bool {
    let is_in_private_use_area = '\u{e000}' <= chr && chr <= '\u{f8ff}'
        || '\u{f0000}' <= chr && chr <= '\u{ffffd}'
        || '\u{100000}' <= chr && chr <= '\u{10fffd}';

    !is_in_private_use_area && !chr.is_ascii_control()
}

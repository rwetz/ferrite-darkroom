//! A pane that scrolls both ways, for a print bigger than the room it has:
//! Ferrite's vertical scrollbar on the right and a matching horizontal one
//! along the bottom, each shown only when that axis overflows.
//!
//! Ferrite ships the vertical bar only; the horizontal one here is the same
//! design turned on its side (track `surface` with a hairline, square
//! `line_strong` thumb, `fg_faint` on hover, amber while dragged).

use ferrite_design::components::scroll::{WIDTH, thumb};
use ferrite_design::prelude::*;
use gpui::{
    AnyElement, App, Bounds, ElementId, HitboxBehavior, InteractiveElement, IntoElement, MouseButton, MouseDownEvent, MouseMoveEvent, ParentElement,
    Pixels, RenderOnce, ScrollHandle, StatefulInteractiveElement, Styled, Window, canvas, div, fill, point, px, size,
};

/// Content `w`×`h` (logical pixels) in at most `room_w`×`room_h`: as big
/// as the content where it fits, scrolling with bars where it doesn't.
#[derive(IntoElement)]
pub struct Pan {
    id: ElementId,
    w: f32,
    h: f32,
    room_w: f32,
    room_h: f32,
    child: AnyElement,
}

pub fn pan(id: impl Into<ElementId>, w: f32, h: f32, room_w: f32, room_h: f32, child: impl IntoElement) -> Pan {
    Pan { id: id.into(), w, h, room_w, room_h, child: child.into_any_element() }
}

/// Which axes overflow, counting the room a bar on the other axis takes.
pub fn overflow(w: f32, h: f32, room_w: f32, room_h: f32) -> (bool, bool) {
    let bar = f32::from(WIDTH);
    let mut x = w > room_w + 0.5;
    let mut y = h > room_h + 0.5;
    // A bar on one axis can push the other over.
    if x && !y {
        y = h > room_h - bar + 0.5;
    }
    if y && !x {
        x = w > room_w - bar + 0.5;
    }
    (x, y)
}

/// The visible part of content `w`×`h` in the room: an axis that
/// overflows is cut to the room, less the other axis's bar if it has one.
pub fn viewport(w: f32, h: f32, room_w: f32, room_h: f32) -> (f32, f32) {
    let (ox, oy) = overflow(w, h, room_w, room_h);
    let bar = f32::from(WIDTH);
    let vw = if ox { (room_w - if oy { bar } else { 0. }).max(1.) } else { w };
    let vh = if oy { (room_h - if ox { bar } else { 0. }).max(1.) } else { h };
    (vw, vh)
}

impl RenderOnce for Pan {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let (ox, oy) = overflow(self.w, self.h, self.room_w, self.room_h);
        if !ox && !oy {
            return div().child(self.child);
        }
        let handle = window.use_keyed_state(self.id.clone(), cx, |_, _| ScrollHandle::new()).read(cx).clone();
        let bar = f32::from(WIDTH);
        let (vw, vh) = viewport(self.w, self.h, self.room_w, self.room_h);
        div()
            .relative()
            .w(px(vw + if oy { bar } else { 0. }))
            .h(px(vh + if ox { bar } else { 0. }))
            .child(
                div()
                    .id(self.id.clone())
                    .w(px(vw))
                    .h(px(vh))
                    .overflow_scroll()
                    .track_scroll(&handle)
                    .child(div().flex_none().w(px(self.w)).h(px(self.h)).child(self.child)),
            )
            .when(oy, |el| el.child(div().absolute().top_0().right_0().w(WIDTH).h(px(vh)).child(scrollbar("pan-v", &handle))))
            .when(ox, |el| el.child(hbar("pan-h", &handle).w(px(vw))))
    }
}

#[derive(Default)]
struct BarState {
    /// Grab point within the thumb while dragging.
    drag: Option<f32>,
    hovered: bool,
}

/// The offset that puts the thumb's left edge at `left` along the track.
fn offset_for(left: f32, track: f32, len: f32, max_offset: f32) -> f32 {
    let travel = (track - len).max(1.);
    (left / travel).clamp(0., 1.) * max_offset
}

/// A horizontal scrollbar for `handle`, pinned to its parent's bottom left.
#[derive(IntoElement)]
pub struct HBar {
    id: ElementId,
    handle: ScrollHandle,
    style: gpui::StyleRefinement,
}

pub fn hbar(id: impl Into<ElementId>, handle: &ScrollHandle) -> HBar {
    HBar { id: id.into(), handle: handle.clone(), style: Default::default() }
}

impl Styled for HBar {
    fn style(&mut self) -> &mut gpui::StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for HBar {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state(self.id.clone(), cx, |_, _| BarState::default());
        let handle = self.handle;
        let mut outer = div();
        *outer.style() = self.style;
        outer.id(self.id).absolute().left_0().bottom_0().h(WIDTH).child(
            canvas(
                |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
                move |bounds: Bounds<Pixels>, hitbox, window, cx| {
                    let p = palette(cx);
                    let track = f32::from(bounds.size.width);
                    let viewport = f32::from(handle.bounds().size.width);
                    let max = f32::from(handle.max_offset().x);
                    let Some((left, len)) = thumb(track, viewport, max, -f32::from(handle.offset().x)) else {
                        return;
                    };
                    let (dragging, hovered) = {
                        let s = state.read(cx);
                        (s.drag.is_some(), s.hovered)
                    };

                    window.paint_quad(fill(bounds, hsla(p.surface)));
                    window.paint_quad(fill(Bounds::new(bounds.origin, size(bounds.size.width, px(1.))), hsla(p.line)));
                    let ink = if dragging { p.accent } else if hovered { p.fg_faint } else { p.line_strong };
                    window.paint_quad(fill(
                        Bounds::new(point(bounds.left() + px(left), bounds.top() + px(2.)), size(px(len), bounds.size.height - px(3.))),
                        hsla(ink),
                    ));

                    let apply = {
                        let handle = handle.clone();
                        move |x: Pixels, grab: f32, window: &mut Window| {
                            let at = f32::from(x - bounds.left()) - grab;
                            let off = offset_for(at, track, len, max);
                            handle.set_offset(point(px(-off), handle.offset().y));
                            window.refresh();
                        }
                    };
                    {
                        let state = state.clone();
                        let hitbox = hitbox.clone();
                        let apply = apply.clone();
                        window.on_mouse_event(move |ev: &MouseDownEvent, phase, window, cx| {
                            if !phase.bubble() || ev.button != MouseButton::Left || !hitbox.is_hovered(window) {
                                return;
                            }
                            cx.stop_propagation();
                            window.prevent_default();
                            window.capture_pointer(hitbox.id);
                            let x = f32::from(ev.position.x - bounds.left());
                            // On the thumb: grab where clicked. On the track: centre the thumb there.
                            let grab = if (left..left + len).contains(&x) { x - left } else { len / 2. };
                            state.update(cx, |s, _| s.drag = Some(grab));
                            apply(ev.position.x, grab, window);
                        });
                    }
                    window.on_mouse_event(move |ev: &MouseMoveEvent, phase, window, cx| {
                        if !phase.bubble() {
                            return;
                        }
                        let over = hitbox.is_hovered(window);
                        let (drag, was_over) = {
                            let s = state.read(cx);
                            (s.drag, s.hovered)
                        };
                        if over != was_over {
                            state.update(cx, |s, _| s.hovered = over);
                            window.refresh();
                        }
                        let Some(grab) = drag else { return };
                        if ev.pressed_button != Some(MouseButton::Left) {
                            state.update(cx, |s, _| s.drag = None);
                            window.release_pointer();
                            window.refresh();
                            return;
                        }
                        apply(ev.position.x, grab, window);
                    });
                },
            )
            .size_full(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bars_only_where_needed() {
        assert_eq!(overflow(500., 400., 600., 600.), (false, false));
        assert_eq!(overflow(1600., 400., 1488., 800.), (true, false));
        assert_eq!(overflow(500., 900., 600., 800.), (false, true));
        // Only just fits wide; the vertical bar pushes it over.
        assert_eq!(overflow(595., 900., 600., 800.), (true, true));
    }

    #[test]
    fn the_viewport_fits_the_room() {
        let bar = f32::from(WIDTH);
        // Tall: cut to the room's height, full width (the bar sits beside it).
        assert_eq!(viewport(500., 1400., 1500., 1000.), (500., 1000.));
        // Wide: cut to the room's width.
        assert_eq!(viewport(1600., 400., 1488., 900.), (1488., 400.));
        // Both: each less the other's bar, and never past the room.
        let (vw, vh) = viewport(3000., 3000., 1500., 1000.);
        assert_eq!((vw, vh), (1500. - bar, 1000. - bar));
        // Fits: as is.
        assert_eq!(viewport(400., 300., 1500., 1000.), (400., 300.));
    }

    #[test]
    fn dragging_maps_back_to_offsets() {
        assert_eq!(offset_for(0., 300., 75., 900.), 0.);
        assert_eq!(offset_for(225., 300., 75., 900.), 900.);
    }
}

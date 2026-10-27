//! 指针事件：移动更新悬停，按下捕获并给焦点，抬起时还在捕获的元素上就算一次点击。

use super::Interaction;
use crate::ui::Response;

impl<M: Clone> Interaction<M> {
    pub(super) fn pointer_moved(&mut self, pointer: Option<(f32, f32)>) -> Response<M> {
        self.pointer = pointer;
        let mut response = Response::new();
        response.changed = self.rehover();
        if self.drag_scroll_bar() {
            response.changed = true;
        }
        response
    }

    /// 正按住滚动条滑块：按指针位置换算新的滚动位置。返回滚动位置有没有变。
    fn drag_scroll_bar(&mut self) -> bool {
        let (Some(pressed), Some((_, pointer_y))) = (self.pressed, self.pointer) else {
            return false;
        };
        let Some(grab) = self.drag_grab else {
            return false;
        };
        let Some(element) = self.element(pressed) else {
            return false;
        };
        let Some(scroller) = element.interact.scroll_drag else {
            return false;
        };
        let Some(extent) = self.extents.get(&scroller).copied() else {
            return false;
        };
        let (_, _, _, thumb) = element.rect();
        let max = extent.max().1;
        if max <= 0.0 || thumb <= 0.0 {
            return false;
        }
        // 滑块能走多远 = 视口高 - 滑块高；每一像素对应 max / 这段距离 的内容
        let travel = (extent.viewport.1 - thumb).max(1.0);
        let top_in_view = (pointer_y - grab) - element.rect().1;
        let old = self.scroll_offset(scroller);
        let offset = (top_in_view / travel * max).clamp(0.0, max);
        if (offset - old.1).abs() < 0.5 {
            return false;
        }
        self.offsets.insert(scroller, (old.0, offset));
        self.scroll_generation += 1;
        true
    }

    pub(super) fn pointer_down(&mut self) -> Response<M> {
        let mut response = Response::new();
        let path: Vec<usize> = match self.hovered.first() {
            Some(&key) => self.chain(self.index.get(&key).copied()).collect(),
            None => Vec::new(),
        };
        // 最近的带点击消息或拖拽滚动的元素捕获指针；它被禁用就谁都不捕获，不往上漏给外层
        self.pressed = path
            .iter()
            .map(|&position| &self.elements[position])
            .find(|element| {
                element.interact.click.is_some() || element.interact.scroll_drag.is_some()
            })
            .filter(|element| !element.interact.disabled)
            .map(|element| element.key);
        // 拖滚动条：记下按点相对滑块顶端的偏移，拖动时保持手感
        self.drag_grab = self.pressed.and_then(|key| {
            let element = self.element(key)?;
            element.interact.scroll_drag?;
            let (_, top, _, _) = element.rect();
            self.pointer.map(|(_, y)| y - top)
        });
        let focused = path
            .iter()
            .map(|&position| &self.elements[position])
            .find(|element| element.interact.takes_focus())
            .map(|element| element.key);
        let moved = self.set_focus(focused, &mut response);
        response.changed = self.pressed.is_some() || moved || self.focus_visible;
        self.focus_visible = false;
        response
    }

    pub(super) fn pointer_up(&mut self) -> Response<M> {
        let mut response = Response::new();
        self.drag_grab = None;
        let Some(key) = self.pressed.take() else {
            return response;
        };
        response.changed = true;
        if self.hovered.contains(&key)
            && let Some(message) = self
                .element(key)
                .filter(|element| element.interact.clickable())
                .and_then(|element| element.interact.click.clone())
        {
            response.messages.push(message);
        }
        response
    }
}

use std::collections::BTreeMap;
use zellij_tile::prelude::*;

#[derive(Default)]
struct State {
    tabs: Vec<TabInfo>,
    panes: PaneManifest,
}

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
        request_permission(&[
            PermissionType::ReadApplicationState,
            PermissionType::ReadCliPipes,
        ]);
        subscribe(&[EventType::TabUpdate, EventType::PaneUpdate]);
    }

    fn update(&mut self, event: Event) -> bool {
        match event {
            Event::TabUpdate(tabs) => self.tabs = tabs,
            Event::PaneUpdate(panes) => self.panes = panes,
            _ => {}
        }
        false
    }

    fn pipe(&mut self, pipe_message: PipeMessage) -> bool {
        let pipe_id = match &pipe_message.source {
            PipeSource::Cli(pipe_id) => Some(pipe_id.clone()),
            _ => None,
        };

        let direction = pipe_message
            .payload
            .as_deref()
            .or_else(|| pipe_message.args.get("direction").map(String::as_str))
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();

        let Some(direction) = parse_direction(&direction) else {
            if let Some(pipe_id) = pipe_id.as_deref() {
                cli_pipe_output(pipe_id, "error: expected one of left/right/up/down\n");
                unblock_cli_pipe_input(pipe_id);
            }
            return false;
        };

        let active_tab = self
            .tabs
            .iter()
            .find(|tab| tab.active)
            .map(|tab| tab.position)
            .unwrap_or(0);

        let result = if let Some(pane) = get_focused_pane(active_tab, &self.panes) {
            if is_on_edge(&pane, direction, &self.panes, active_tab) {
                "edge\n"
            } else {
                "inside\n"
            }
        } else {
            "edge\n"
        };

        if let Some(pipe_id) = pipe_id.as_deref() {
            cli_pipe_output(pipe_id, result);
            unblock_cli_pipe_input(pipe_id);
        }
        false
    }
}

fn parse_direction(s: &str) -> Option<Direction> {
    match s {
        "left" | "l" | "west" => Some(Direction::Left),
        "right" | "r" | "east" => Some(Direction::Right),
        "up" | "u" | "north" => Some(Direction::Up),
        "down" | "d" | "south" => Some(Direction::Down),
        _ => None,
    }
}

fn is_on_edge(
    focused: &PaneInfo,
    direction: Direction,
    manifest: &PaneManifest,
    tab_position: usize,
) -> bool {
    // Fullscreen or floating panes have no useful tiled neighbour for kitty-style handoff.
    if focused.is_fullscreen || focused.is_floating {
        return true;
    }

    let Some(panes) = manifest.panes.get(&tab_position) else {
        return true;
    };

    !panes.iter().any(|candidate| {
        candidate.is_selectable
            && !candidate.is_suppressed
            && !candidate.is_floating
            && !(candidate.id == focused.id && candidate.is_plugin == focused.is_plugin)
            && touches_in_direction(focused, candidate, direction)
    })
}

fn touches_in_direction(a: &PaneInfo, b: &PaneInfo, direction: Direction) -> bool {
    let ax1 = a.pane_x;
    let ay1 = a.pane_y;
    let ax2 = a.pane_x + a.pane_columns;
    let ay2 = a.pane_y + a.pane_rows;

    let bx1 = b.pane_x;
    let by1 = b.pane_y;
    let bx2 = b.pane_x + b.pane_columns;
    let by2 = b.pane_y + b.pane_rows;

    match direction {
        Direction::Left => bx2 == ax1 && ranges_overlap(ay1, ay2, by1, by2),
        Direction::Right => bx1 == ax2 && ranges_overlap(ay1, ay2, by1, by2),
        Direction::Up => by2 == ay1 && ranges_overlap(ax1, ax2, bx1, bx2),
        Direction::Down => by1 == ay2 && ranges_overlap(ax1, ax2, bx1, bx2),
    }
}

fn ranges_overlap(a1: usize, a2: usize, b1: usize, b2: usize) -> bool {
    a1 < b2 && b1 < a2
}

#[no_mangle]
pub extern "C" fn _start() {}

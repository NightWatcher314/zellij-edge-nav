use std::collections::BTreeMap;
use zellij_tile::prelude::*;

#[derive(Default)]
struct State {
    tabs: Vec<TabInfo>,
    panes: PaneManifest,
    handoff_command: String,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum OutputMode {
    Words,
    TmuxFlag,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
enum Action {
    Query,
    Move,
}

struct Request {
    direction: Direction,
    output_mode: OutputMode,
    action: Action,
}

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self, configuration: BTreeMap<String, String>) {
        self.handoff_command = configuration
            .get("handoff_command")
            .or_else(|| configuration.get("handoff-command"))
            .cloned()
            .unwrap_or_else(|| "kitten @ kitten neighboring_window.py {direction}".to_owned());

        request_permission(&[
            PermissionType::ReadApplicationState,
            PermissionType::ReadCliPipes,
            PermissionType::ChangeApplicationState,
            PermissionType::RunCommands,
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

        let query = pipe_message
            .payload
            .as_deref()
            .or_else(|| pipe_message.args.get("query").map(String::as_str))
            .or_else(|| pipe_message.args.get("format").map(String::as_str))
            .or_else(|| pipe_message.args.get("direction").map(String::as_str))
            .filter(|s| !s.trim().is_empty())
            .unwrap_or(pipe_message.name.as_str())
            .trim();

        let Some(mut request) = parse_request(query) else {
            if let Some(pipe_id) = pipe_id.as_deref() {
                cli_pipe_output(
                    pipe_id,
                    "error: expected left/right/up/down, move:left/right/up/down, or pane_at_left/right/top/bottom\n",
                );
                unblock_cli_pipe_input(pipe_id);
            }
            return false;
        };

        if message_requests_move(&pipe_message) {
            request.action = Action::Move;
        }

        let active_tab = self
            .tabs
            .iter()
            .find(|tab| tab.active)
            .map(|tab| tab.position)
            .unwrap_or(0);

        let at_edge = self
            .focused_pane_at_edge(active_tab, request.direction)
            .unwrap_or(true);

        if request.action == Action::Move {
            if at_edge {
                hand_off_to_outer_nav(request.direction, &self.handoff_command);
            } else {
                move_focus(request.direction);
            }
        }

        let result = match request.output_mode {
            OutputMode::Words => {
                if at_edge {
                    "edge\n"
                } else {
                    "inside\n"
                }
            }
            OutputMode::TmuxFlag => {
                if at_edge {
                    "1\n"
                } else {
                    "0\n"
                }
            }
        };

        if let Some(pipe_id) = pipe_id.as_deref() {
            cli_pipe_output(pipe_id, result);
            unblock_cli_pipe_input(pipe_id);
        }
        false
    }
}

impl State {
    fn focused_pane_at_edge(&self, tab_position: usize, direction: Direction) -> Option<bool> {
        let pane = get_focused_pane(tab_position, &self.panes)?;
        Some(is_on_edge(&pane, direction, &self.panes, tab_position))
    }
}

fn parse_request(raw: &str) -> Option<Request> {
    let mut action = Action::Query;
    let mut query = raw
        .trim()
        .trim_matches('\'')
        .trim_matches('"')
        .trim_start_matches("#{")
        .trim_end_matches('}')
        .trim()
        .to_ascii_lowercase();

    if let Some(direction) = query
        .strip_prefix("move:")
        .or_else(|| query.strip_prefix("move-"))
        .or_else(|| query.strip_prefix("move_"))
        .or_else(|| query.strip_prefix("nav:"))
        .or_else(|| query.strip_prefix("navigate:"))
    {
        action = Action::Move;
        query = direction.trim().to_owned();
    }

    if let Some(direction) = parse_direction(&query) {
        return Some(Request {
            direction,
            output_mode: OutputMode::Words,
            action,
        });
    }

    let edge = query
        .strip_prefix("pane_at_")
        .or_else(|| query.strip_prefix("@pane_at_"))?;
    let direction = match edge {
        "left" | "l" | "west" => Direction::Left,
        "right" | "r" | "east" => Direction::Right,
        "top" | "up" | "u" | "north" => Direction::Up,
        "bottom" | "down" | "d" | "south" => Direction::Down,
        _ => return None,
    };

    Some(Request {
        direction,
        output_mode: OutputMode::TmuxFlag,
        action,
    })
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

fn message_requests_move(pipe_message: &PipeMessage) -> bool {
    [
        Some(pipe_message.name.as_str()),
        pipe_message.args.get("action").map(String::as_str),
    ]
    .into_iter()
    .flatten()
    .any(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "move" | "nav" | "navigate"
        )
    })
}

fn hand_off_to_outer_nav(direction: Direction, handoff_command: &str) {
    let command = if handoff_command.contains("{direction}") {
        handoff_command.replace("{direction}", direction_name(direction))
    } else {
        format!("{} {}", handoff_command, direction_name(direction))
    };
    run_command(&["sh", "-lc", command.as_str()], BTreeMap::new());
}

fn direction_name(direction: Direction) -> &'static str {
    match direction {
        Direction::Left => "left",
        Direction::Right => "right",
        Direction::Up => "up",
        Direction::Down => "down",
    }
}

#[no_mangle]
pub extern "C" fn _start() {}

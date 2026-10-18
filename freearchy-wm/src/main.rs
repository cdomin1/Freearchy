//! freearchy-wm — Freearchy's X11 tiling window manager.
//!
//! Built on penrose (pure-Rust x11rb backend). Keybinds follow the
//! Omarchy/Hyprland muscle-memory map: Super+h/j/k/l focus, Super+Enter
//! terminal, Super+Space launcher, workspaces 1–9.
//!
//! Static config (compile-time) for now — a parsed config file is a
//! deliberate later step; keep the defaults matching config/freearchy-wm/*.

use penrose::{
    builtin::{
        actions::{
            exit,
            floating::{MouseDragHandler, MouseResizeHandler, sink_focused},
            modify_with, send_layout_message, spawn,
        },
        layout::messages::{ExpandMain, IncMain, ShrinkMain},
    },
    core::{
        Config, WindowManager,
        bindings::{
            KeyEventHandler, MouseEventHandler, MouseState, click_handler,
            parse_keybindings_with_xmodmap,
        },
    },
    map,
    x11rb::RustConn,
    Result,
};

use std::collections::HashMap;

const TERMINAL: &str = "foot";      // falls back to xterm if absent
const LAUNCHER: &str = "rofi -show drun";

fn raw_key_bindings() -> HashMap<String, Box<dyn KeyEventHandler<RustConn>>> {
    let mut raw_bindings = map! {
        map_keys: |k: &str| k.to_string();

        // focus (vim keys + arrows)
        "M-j" => modify_with(|cs| cs.focus_down()),
        "M-k" => modify_with(|cs| cs.focus_up()),
        "M-Down" => modify_with(|cs| cs.focus_down()),
        "M-Up" => modify_with(|cs| cs.focus_up()),
        "M-h" => send_layout_message(|| IncMain(-1)),
        "M-l" => send_layout_message(|| IncMain(1)),

        // swap / move
        "M-S-j" => modify_with(|cs| cs.swap_down()),
        "M-S-k" => modify_with(|cs| cs.swap_up()),
        "M-S-Down" => modify_with(|cs| cs.swap_down()),
        "M-S-Up" => modify_with(|cs| cs.swap_up()),

        // layout sizing (Omarchy: Super +/- resize)
        "M-equal" => send_layout_message(|| ExpandMain),
        "M-minus" => send_layout_message(|| ShrinkMain),

        // workspace / window cycling
        "M-Tab" => modify_with(|cs| cs.toggle_tag()),
        "M-bracketright" => modify_with(|cs| cs.next_screen()),
        "M-bracketleft" => modify_with(|cs| cs.previous_screen()),
        "M-grave" => modify_with(|cs| cs.next_layout()),
        "M-S-grave" => modify_with(|cs| cs.previous_layout()),

        // window ops
        "M-q" => modify_with(|cs| cs.kill_focused()),
        "M-S-f" => penrose::extensions::actions::toggle_fullscreen(),

        // apps — Omarchy defaults
        "M-Return" => spawn(TERMINAL),
        "M-space" => spawn(LAUNCHER),
        "M-S-b" => spawn("firefox"),
        "M-b" => spawn("firefox"),
        "M-f" => spawn("nemo"),
        "M-t" => spawn(TERMINAL),

        // session
        "M-S-q" => exit(),
    };

    for tag in &["1", "2", "3", "4", "5", "6", "7", "8", "9"] {
        raw_bindings.extend([
            (
                format!("M-{tag}"),
                modify_with(move |cs| cs.focus_tag(tag)),
            ),
            (
                format!("M-S-{tag}"),
                modify_with(move |cs| cs.move_focused_to_tag(tag)),
            ),
        ]);
    }

    raw_bindings
}

fn mouse_bindings() -> HashMap<MouseState, Box<dyn MouseEventHandler<RustConn>>> {
    use penrose::core::bindings::{
        ModifierKey::{Meta, Shift},
        MouseButton::{Left, Middle, Right},
    };

    map! {
        map_keys: |(button, modifiers)| MouseState { button, modifiers };

        (Left, vec![Shift, Meta]) => MouseDragHandler::boxed_default(),
        (Right, vec![Shift, Meta]) => MouseResizeHandler::boxed_default(),
        (Middle, vec![Shift, Meta]) => click_handler(sink_focused()),
    }
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .finish()
        .init();

    let conn = RustConn::new()?;
    let key_bindings = parse_keybindings_with_xmodmap(raw_key_bindings())?;
    let wm = WindowManager::new(Config::default(), key_bindings, mouse_bindings(), conn)?;

    wm.run()
}

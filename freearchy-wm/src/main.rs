//! freearchy-wm — Freearchy's X11 tiling window manager.
//!
//! Built on penrose (pure-Rust x11rb backend). Keybinds follow the
//! Omarchy/Hyprland muscle-memory map: Super+h/j/k/l focus, Super+Enter
//! terminal, Super+Space launcher, workspaces 1–9.
//!
//! Styling follows the Omarchy/Tokyo-Night palette: dark bg, blue accent
//! borders, 5px gaps, and a dwm-style status bar across the top.

use penrose::{
    builtin::{
        actions::{
            exit,
            floating::{MouseDragHandler, MouseResizeHandler, sink_focused},
            modify_with, send_layout_message, spawn,
        },
        layout::{
            MainAndStack, Monocle,
            messages::{ExpandMain, IncMain, ShrinkMain},
            transformers::{Gaps, ReserveTop},
        },
    },
    core::{
        Config, WindowManager,
        bindings::{
            KeyEventHandler, MouseEventHandler, MouseState, click_handler,
            parse_keybindings_with_xmodmap,
        },
        layout::LayoutStack,
    },
    extensions::hooks::add_ewmh_hooks,
    map, stack,
    x11rb::RustConn,
    Result,
};
use penrose_ui::{bar::Position, core::TextStyle, status_bar};
use std::collections::HashMap;

const TERMINAL: &str = "alacritty";
const LAUNCHER: &str = "rofi -show drun";

// --- Tokyo Night (Omarchy's palette) ---------------------------------------
const TN_BG: u32 = 0x1a1b26ff;        // background
const TN_FG: u32 = 0xc0caf5ff;        // foreground text
const TN_ACCENT: u32 = 0x7aa2f7ff;    // blue accent — focused border / active ws
const TN_GREY: u32 = 0x414868ff;      // unfocused border / empty ws

const FONT: &str = "monospace";
const BAR_HEIGHT_PX: u32 = 24;
const BAR_WIDGETS: usize = 8;

const MAX_MAIN: u32 = 1;
const RATIO: f32 = 0.55;
const RATIO_STEP: f32 = 0.1;
const OUTER_PX: u32 = 5;
const INNER_PX: u32 = 5;
const BORDER_PX: u32 = 2;

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
        "M-t" => spawn(TERMINAL),
        "M-space" => spawn(LAUNCHER),
        "M-b" => spawn("firefox"),
        "M-f" => spawn("nemo"),

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

fn layouts() -> LayoutStack {
    stack!(
        MainAndStack::side(MAX_MAIN, RATIO, RATIO_STEP),
        MainAndStack::side_mirrored(MAX_MAIN, RATIO, RATIO_STEP),
        MainAndStack::bottom(MAX_MAIN, RATIO, RATIO_STEP),
        Monocle::boxed()
    )
    .map(|layout| ReserveTop::wrap(Gaps::wrap(layout, OUTER_PX, INNER_PX), BAR_HEIGHT_PX))
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .finish()
        .init();

    let config = add_ewmh_hooks(Config {
        border_width: BORDER_PX,
        focused_border: TN_ACCENT,
        unfocused_border: TN_GREY,
        default_layouts: layouts(),
        ..Config::default()
    });

    let conn = RustConn::new()?;
    let key_bindings = parse_keybindings_with_xmodmap(raw_key_bindings())?;

    let style = TextStyle {
        fg: TN_FG.into(),
        bg: Some(TN_BG.into()),
        padding: (4, 4),
    };
    let bar = status_bar(
        BAR_HEIGHT_PX,
        FONT,
        BAR_WIDGETS,
        style,
        TN_ACCENT,
        TN_GREY,
        Position::Top,
    )
    .expect("failed to create status bar");

    let wm = bar.add_to(WindowManager::new(
        config,
        key_bindings,
        mouse_bindings(),
        conn,
    )?);

    wm.run()
}

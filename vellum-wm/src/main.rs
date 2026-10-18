//! vellum-wm — Vellum's X11 tiling window manager.
//!
//! Built on penrose (pure-Rust x11rb backend). Keybinds follow the
//! Omarchy/Hyprland muscle-memory map: Super+h/j/k/l focus, Super+Enter
//! terminal, Super+Space launcher, workspaces 1–9.
//!
//! Styling follows the Omarchy/Tokyo-Night palette: dark bg, blue accent
//! borders, 5px gaps, and a polybar panel across the top (ReserveTop
//! keeps tiled windows out from under it).

use penrose::{
    builtin::{
        actions::{
            exit,
            floating::{MouseDragHandler, MouseResizeHandler, float_focused, sink_focused},
            key_handler, modify_with, send_layout_message, spawn,
        },
        layout::{
            MainAndStack, Monocle,
            messages::{ExpandMain, IncMain, ShrinkMain},
            transformers::{Gaps, ReserveTop},
        },
    },
    core::{
        Config, State, WindowManager,
        bindings::{
            KeyEventHandler, MouseEventHandler, MouseState, click_handler,
            parse_keybindings_with_xmodmap,
        },
        layout::LayoutStack,
    },
    extensions::{
        actions::toggle_fullscreen,
        hooks::{
            add_ewmh_hooks,
            named_scratchpads::{add_named_scratchpads, NamedScratchPad},
        },
    },
    map, stack,
    util::spawn_with_args,
    x::query::ClassName,
    x11rb::RustConn,
    Result, Xid,
};
use std::collections::HashMap;
use tracing_subscriber::util::SubscriberInitExt;

const TERMINAL: &str = "alacritty";
const LAUNCHER: &str = "rofi -show drun";
const MENU: &str = "vellum-menu";

// --- theme colors (Tokyo Night defaults; overridden by the active theme's
//     wm.conf at ~/.config/vellum/current/theme) ----------------------------
const TN_ACCENT: u32 = 0x7aa2f7ff;    // blue accent — focused border
const TN_GREY: u32 = 0x414868ff;      // unfocused border

const BAR_HEIGHT_PX: u32 = 28; // reserved for polybar

/// penrose's `spawn` execs argv split on whitespace — no shell. Use this for
/// commands that need `$VAR`/`$(...)` expansion.
fn spawn_sh(cmd: &'static str) -> Box<dyn KeyEventHandler<RustConn>> {
    key_handler(move |_, _| spawn_with_args("sh", &["-c", cmd]))
}

/// Parse `#rrggbb` -> 0xRRGGBBAA (opaque). None on anything else.
fn hex_color(s: &str) -> Option<u32> {
    let s = s.trim().strip_prefix('#')?;
    u32::from_str_radix(s, 16).ok().filter(|_| s.len() == 6).map(|v| v << 8 | 0xff)
}

/// Read `focused`/`unfocused` hex colors from the theme symlink's wm.conf.
fn theme_borders() -> (u32, u32) {
    let home = std::env::var("HOME").unwrap_or_default();
    let path = format!("{home}/.config/vellum/current/theme/wm.conf");
    let Ok(text) = std::fs::read_to_string(&path) else {
        return (TN_ACCENT, TN_GREY);
    };
    let mut focused = TN_ACCENT;
    let mut unfocused = TN_GREY;
    for line in text.lines() {
        let Some((k, v)) = line.split_once('=') else { continue };
        let v = hex_color(v);
        match (k.trim(), v) {
            ("focused", Some(c)) => focused = c,
            ("unfocused", Some(c)) => unfocused = c,
            _ => {}
        }
    }
    (focused, unfocused)
}

const MAX_MAIN: u32 = 1;
const RATIO: f32 = 0.55;
const RATIO_STEP: f32 = 0.1;
const OUTER_PX: u32 = 5;
const INNER_PX: u32 = 5;
const BORDER_PX: u32 = 2;

/// Centre-floating rect (75% of screen) for the scratchpad terminal.
fn centre_float(id: Xid, state: &mut State<RustConn>, _: &RustConn) -> Result<()> {
    let r = state.client_set.current_screen().geometry();
    state.client_set.float(id, r.shrink_in(r.w / 8))
}

fn raw_key_bindings(
    toggle_nsp: impl KeyEventHandler<RustConn> + 'static,
) -> HashMap<String, Box<dyn KeyEventHandler<RustConn>>> {
    let mut raw_bindings = map! {
        map_keys: |k: &str| k.to_string();

        // focus (vim keys + arrows; stack order — penrose has no spatial
        // focus, so h/l adjust the main/stack split instead)
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
        "M-C-h" => send_layout_message(|| ShrinkMain),
        "M-C-l" => send_layout_message(|| ExpandMain),

        // workspace / window cycling (Omarchy: Super+Ctrl+arrows)
        "M-Tab" => modify_with(|cs| cs.toggle_tag()),
        "M-C-Right" => modify_with(|cs| cs.focus_next_workspace()),
        "M-C-Left"  => modify_with(|cs| cs.focus_previous_workspace()),
        "M-bracketright" => modify_with(|cs| cs.next_screen()),
        "M-bracketleft" => modify_with(|cs| cs.previous_screen()),
        "M-grave" => modify_with(|cs| cs.next_layout()),
        "M-S-grave" => modify_with(|cs| cs.previous_layout()),

        // window ops (Omarchy: Super+F fullscreen, Super+Shift+F files,
        // Super+V float, Super+W/Q close)
        "M-q" => modify_with(|cs| cs.kill_focused()),
        "M-w" => modify_with(|cs| cs.kill_focused()),
        "M-f" => toggle_fullscreen(),
        "M-v" => float_focused(),
        "M-S-v" => sink_focused(),

        // apps — Omarchy muscle memory
        "M-Return" => spawn(TERMINAL),
        "M-space" => spawn(LAUNCHER),
        "M-A-space" => spawn(MENU),                       // Omarchy: Super+Alt+Space
        "M-b" => spawn("firefox"),
        "M-S-b" => spawn("firefox --private-window"),
        "M-S-f" => spawn("thunar"),
        "M-n" => spawn("alacritty -e nvim"),              // editor
        "M-t" => spawn("alacritty -e btop"),              // activity monitor
        "M-y" => spawn("alacritty -e yazi"),              // TUI file manager
        "M-g" => spawn("alacritty -e lazygit"),
        "M-a" => spawn_sh("command -v aichat >/dev/null 2>&1 \
            && exec alacritty -e aichat || exec rofi -e 'aichat not installed'"),
        "M-c" => spawn("rofi -show calc -modi calc -no-show-match -no-sort"),
        "M-slash" => spawn("vellum-menu keys"),           // keybinding sheet

        // scratchpad (Omarchy: Super+S toggles a special workspace)
        "M-s" => Box::new(toggle_nsp),

        // session (Omarchy: Super+Esc menu, Ctrl+Alt+Del logout)
        "M-Escape" => spawn("vellum-menu system"),
        "M-S-Escape" => spawn("doas zzz"),                // suspend (nopass doas)
        "C-A-l" => spawn("slock"),
        "M-S-q" => exit(),

        // capture (Omarchy: Print variants)
        "Print"   => spawn("vellum-shot select"),
        "S-Print" => spawn("vellum-shot window"),
        "C-Print" => spawn("vellum-shot full"),
        "M-Print" => spawn("vellum-shot color"),

        // media / brightness keys — FreeBSD-native tools (mixer(8) needs
        // the % suffix for relative steps; backlight(8) takes incr/decr)
        "XF86AudioRaiseVolume" => spawn("mixer vol=+5%"),
        "XF86AudioLowerVolume" => spawn("mixer vol=-5%"),
        "XF86AudioMute"        => spawn("mixer vol.mute=toggle"),
        "XF86AudioMicMute"     => spawn("mixer mic.mute=toggle"),
        "XF86AudioPlay"        => spawn("playerctl play-pause"),
        "XF86AudioNext"        => spawn("playerctl next"),
        "XF86AudioPrev"        => spawn("playerctl previous"),
        "XF86MonBrightnessUp"   => spawn("backlight incr 10"),
        "XF86MonBrightnessDown" => spawn("backlight decr 10"),
        "XF86Calculator"        => spawn("rofi -show calc -modi calc -no-show-match -no-sort"),
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

    let (focused, unfocused) = theme_borders();
    let config = add_ewmh_hooks(Config {
        border_width: BORDER_PX,
        focused_border: focused.into(),
        normal_border: unfocused.into(),
        default_layouts: layouts(),
        ..Config::default()
    });

    let conn = RustConn::new()?;

    // Super+S scratchpad terminal — floats centred, hides to an invisible
    // workspace when toggled off.
    let (nsp, toggle_nsp) = NamedScratchPad::new(
        "terminal",
        "alacritty --class vellum-scratch -e sh",
        ClassName("vellum-scratch"),
        centre_float,
        true,
    );

    let key_bindings = parse_keybindings_with_xmodmap(raw_key_bindings(toggle_nsp))?;

    let wm = WindowManager::new(config, key_bindings, mouse_bindings(), conn)?;
    add_named_scratchpads(wm, vec![nsp]).run()
}

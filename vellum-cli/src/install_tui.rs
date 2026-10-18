//! Ratatui wizard for `vellum install-system` — the Omarchy-style
//! installer UI. Drives an InstallSpec out of keyboard/user/disk/confirm
//! screens, streams the pipeline log, then offers reboot.

use anyhow::{bail, Result};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::layout::{Alignment, Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph};
use ratatui::{DefaultTerminal, Frame};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use super::install_system::{disks, execute, InstallSpec};
use super::util;

const GREEN: Color = Color::Indexed(120);
const BLUE: Color = Color::Indexed(111);
const DIM: Color = Color::Indexed(240);
const FG: Color = Color::Indexed(255);

/// Pixel-style banner matching branding/vellum-logo.svg.
const LOGO: [&str; 5] = [
    "██   ██ ███████ ██      ██      ██   ██ ███   ███",
    "██   ██ ██      ██      ██      ██   ██ ████ ████",
    " ██ ██  █████   ██      ██      ██   ██ ██ ███ ██",
    " ██ ██  ██      ██      ██      ██   ██ ██  █  ██",
    "  ███   ███████ ███████ ███████  █████  ██     ██",
];

const KEYMAPS: [(&str, &str); 14] = [
    ("English (US)", "us"),
    ("English (US, Dvorak)", "us.dvorak"),
    ("Danish", "dk"),
    ("Dutch", "nl"),
    ("Estonian", "ee"),
    ("Finnish", "fi"),
    ("French", "fr"),
    ("French (Canada)", "fr.ca.acc"),
    ("German", "de"),
    ("Italian", "it"),
    ("Norwegian", "no"),
    ("Spanish", "es"),
    ("Swedish", "se"),
    ("Swiss German", "ch"),
];

#[derive(Clone, Copy, PartialEq)]
enum Screen {
    Keyboard,
    Username,
    FullName,
    Password,
    ConfirmPass,
    Email,
    Hostname,
    Disk,
    ConfirmErase,
    Installing,
    Done,
    Failed,
}

enum Msg {
    Log(String),
    Finished(std::result::Result<(), String>),
}

struct Wizard {
    screen: Screen,
    username: String,
    fullname: String,
    password: String,
    confirm: String,
    email: String,
    hostname: String,
    keymap: ListState,
    disk_list: ListState,
    disks: Vec<(String, String)>,
    yes_erase: bool,
    logs: Vec<String>,
    rx: Option<mpsc::Receiver<Msg>>,
    started: Instant,
    elapsed: Duration,
    err: String,
}

fn input_screen(s: Screen) -> bool {
    matches!(
        s,
        Screen::Username
            | Screen::FullName
            | Screen::Password
            | Screen::ConfirmPass
            | Screen::Email
            | Screen::Hostname
    )
}

fn input_mut(w: &mut Wizard) -> &mut String {
    match w.screen {
        Screen::Username => &mut w.username,
        Screen::FullName => &mut w.fullname,
        Screen::Password => &mut w.password,
        Screen::ConfirmPass => &mut w.confirm,
        Screen::Email => &mut w.email,
        _ => &mut w.hostname,
    }
}

/// Runs the wizard end-to-end: prompts, install pipeline, done screen,
/// optional reboot. Returns only on abort or failure.
pub fn run_wizard() -> Result<()> {
    let mut term = ratatui::init();
    let out = wizard_loop(&mut term);
    ratatui::restore();
    out
}

fn wizard_loop(term: &mut DefaultTerminal) -> Result<()> {
    let disk_list = disks()?;
    if disk_list.is_empty() {
        bail!("no candidate disks found (kern.disks empty)");
    }
    let mut w = Wizard {
        screen: Screen::Keyboard,
        username: String::new(),
        fullname: String::new(),
        password: String::new(),
        confirm: String::new(),
        email: String::new(),
        hostname: "vellum".into(),
        keymap: ListState::default().with_selected(Some(0)),
        disk_list: ListState::default().with_selected(Some(0)),
        disks: disk_list,
        yes_erase: false,
        logs: vec![],
        rx: None,
        started: Instant::now(),
        elapsed: Duration::ZERO,
        err: String::new(),
    };

    loop {
        term.draw(|f| draw(f, &mut w))?;
        drain(&mut w);
        if !event::poll(Duration::from_millis(100))? {
            continue;
        }
        if let Event::Key(k) = event::read()? {
            if k.kind != KeyEventKind::Press {
                continue;
            }
            match w.screen {
                Screen::Keyboard => match k.code {
                    KeyCode::Up | KeyCode::Char('k') => sel(&mut w.keymap, KEYMAPS.len(), -1),
                    KeyCode::Down | KeyCode::Char('j') => sel(&mut w.keymap, KEYMAPS.len(), 1),
                    KeyCode::Enter => {
                        // Apply immediately so typing below matches the choice.
                        let _ = util::run(
                            "kbdcontrol",
                            &["-l", KEYMAPS[w.keymap.selected().unwrap_or(0)].1],
                        );
                        w.screen = Screen::Username;
                    }
                    _ => {}
                },
                s if input_screen(s) => match k.code {
                    KeyCode::Enter => {
                        if let Some(next) = input_done(&mut w) {
                            w.screen = next;
                        }
                    }
                    KeyCode::Backspace => {
                        input_mut(&mut w).pop();
                    }
                    KeyCode::Char(c) => input_mut(&mut w).push(c),
                    _ => {}
                },
                Screen::Disk => match k.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        sel(&mut w.disk_list, w.disks.len(), -1)
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        sel(&mut w.disk_list, w.disks.len(), 1)
                    }
                    KeyCode::Enter => w.screen = Screen::ConfirmErase,
                    _ => {}
                },
                Screen::ConfirmErase => match k.code {
                    KeyCode::Left | KeyCode::Char('h') => w.yes_erase = true,
                    KeyCode::Right | KeyCode::Char('l') => w.yes_erase = false,
                    KeyCode::Char('y') => {
                        start_install(&mut w);
                        w.screen = Screen::Installing;
                    }
                    KeyCode::Char('n') => w.screen = Screen::Disk,
                    KeyCode::Enter => {
                        if w.yes_erase {
                            start_install(&mut w);
                            w.screen = Screen::Installing;
                        } else {
                            w.screen = Screen::Disk;
                        }
                    }
                    _ => {}
                },
                Screen::Done => {
                    if k.code == KeyCode::Enter {
                        let _ = std::process::Command::new("shutdown")
                            .args(["-r", "now"])
                            .status();
                        return Ok(());
                    }
                }
                Screen::Failed | Screen::Installing => {}
                _ => {}
            }
            if k.code == KeyCode::Esc
                && !matches!(w.screen, Screen::Installing | Screen::Done)
            {
                bail!("aborted");
            }
            if k.code == KeyCode::Char('q')
                && matches!(w.screen, Screen::Done | Screen::Failed)
            {
                bail!("aborted");
            }
        }
    }
}

fn input_done(w: &mut Wizard) -> Option<Screen> {
    let ok = match w.screen {
        Screen::Username => {
            !w.username.is_empty()
                && w.username
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        }
        Screen::Password => !w.password.is_empty(),
        Screen::ConfirmPass => w.confirm == w.password,
        Screen::Hostname => !w.hostname.is_empty(),
        _ => true, // FullName and Email are skippable
    };
    if !ok {
        return None;
    }
    Some(match w.screen {
        Screen::Username => Screen::FullName,
        Screen::FullName => Screen::Password,
        Screen::Password => Screen::ConfirmPass,
        Screen::ConfirmPass => Screen::Email,
        Screen::Email => Screen::Hostname,
        _ => Screen::Disk,
    })
}

fn sel(state: &mut ListState, len: usize, dir: i32) {
    let cur = state.selected().unwrap_or(0) as i32;
    let next = (cur + dir).rem_euclid(len.max(1) as i32) as usize;
    state.select(Some(next));
}

fn start_install(w: &mut Wizard) {
    let (tx, rx) = mpsc::channel();
    w.rx = Some(rx);
    w.started = Instant::now();
    let spec = InstallSpec {
        disk: w.disks[w.disk_list.selected().unwrap_or(0)].0.clone(),
        hostname: w.hostname.clone(),
        username: w.username.clone(),
        fullname: Some(w.fullname.clone()),
        email: if w.email.is_empty() { None } else { Some(w.email.clone()) },
        keymap: KEYMAPS[w.keymap.selected().unwrap_or(0)].1.to_string(),
        password: Some(w.password.clone()),
    };
    thread::spawn(move || {
        let log = |m: &str| {
            let _ = tx.send(Msg::Log(m.to_string()));
        };
        let res = execute(&spec, &log).map_err(|e| format!("{e:#}"));
        let _ = tx.send(Msg::Finished(res));
    });
}

fn drain(w: &mut Wizard) {
    let Some(rx) = &w.rx else { return };
    while let Ok(msg) = rx.try_recv() {
        match msg {
            Msg::Log(l) => w.logs.push(l),
            Msg::Finished(Ok(())) => {
                w.elapsed = w.started.elapsed();
                w.screen = Screen::Done;
            }
            Msg::Finished(Err(e)) => {
                w.err = e;
                w.screen = Screen::Failed;
            }
        }
    }
}

fn draw(f: &mut Frame, w: &mut Wizard) {
    let area = f.area();
    let col = Layout::vertical([
        Constraint::Length(5 + 2), // logo
        Constraint::Length(2),     // subtitle
        Constraint::Min(8),        // body
        Constraint::Length(2),     // footer
    ])
    .split(area);

    let color = if w.screen == Screen::Done { BLUE } else { GREEN };
    let logo: Vec<Line> = LOGO
        .iter()
        .map(|l| {
            Line::from(Span::styled(
                *l,
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ))
        })
        .collect();
    f.render_widget(
        Paragraph::new(logo).alignment(Alignment::Center),
        Layout::horizontal([Constraint::Percentage(100)]).split(col[0])[0],
    );

    let subtitle = match w.screen {
        Screen::Keyboard => "Let's setup your machine...",
        Screen::Username | Screen::FullName | Screen::Password
        | Screen::ConfirmPass | Screen::Email => "Let's setup your user account...",
        Screen::Hostname => "Name your machine...",
        Screen::Disk | Screen::ConfirmErase => "Let's select where to install Vellum...",
        Screen::Installing => "Installing Vellum...",
        Screen::Done => "",
        Screen::Failed => "Install failed.",
    };
    f.render_widget(
        Paragraph::new(subtitle)
            .alignment(Alignment::Center)
            .style(Style::default().fg(FG)),
        col[1],
    );

    let body = col[2];
    match w.screen {
        Screen::Keyboard => {
            let items: Vec<ListItem> = KEYMAPS
                .iter()
                .map(|(n, _)| ListItem::new(*n))
                .collect();
            let list = List::new(items)
                .highlight_style(Style::default().fg(color).add_modifier(Modifier::BOLD))
                .highlight_symbol("> ");
            f.render_stateful_widget(list, centered(body, 44), &mut w.keymap);
        }
        Screen::Disk => {
            let items: Vec<ListItem> = w
                .disks
                .iter()
                .map(|(_, l)| ListItem::new(l.clone()))
                .collect();
            let list = List::new(items)
                .highlight_style(Style::default().fg(color).add_modifier(Modifier::BOLD))
                .highlight_symbol("> ");
            f.render_stateful_widget(list, centered(body, 44), &mut w.disk_list);
        }
        s if input_screen(s) => {
            let (prompt, hint, value, mask) = match s {
                Screen::Username => ("Username", "alphanumeric without spaces (like dhh)", &w.username, false),
                Screen::FullName => ("Full name", "", &w.fullname, false),
                Screen::Password => ("Password", "Used for user + root", &w.password, true),
                Screen::ConfirmPass => ("Confirm", "", &w.confirm, true),
                Screen::Email => ("Email address", "Used for git authentication (hit return to skip)", &w.email, false),
                _ => ("Hostname", "lower-case hostname", &w.hostname, false),
            };
            let shown = if mask { "•".repeat(value.chars().count()) } else { value.clone() };
            let mut lines = vec![
                Line::from(vec![
                    Span::styled(prompt, Style::default().fg(BLUE)),
                    Span::styled("> ", Style::default().fg(BLUE)),
                    Span::styled(shown, Style::default().fg(FG)),
                    Span::styled("█", Style::default().fg(Color::Indexed(177))),
                ]),
            ];
            if !hint.is_empty() {
                lines.push(Line::from(Span::styled(
                    hint.to_string(),
                    Style::default().fg(DIM),
                )));
            }
            if s == Screen::ConfirmPass && !w.confirm.is_empty() && w.confirm != w.password {
                lines.push(Line::from(Span::styled(
                    "passwords don't match",
                    Style::default().fg(Color::Red),
                )));
            }
            f.render_widget(Paragraph::new(lines), centered(body, 44));
        }
        Screen::ConfirmErase => {
            let disk = &w.disks[w.disk_list.selected().unwrap_or(0)].0;
            let yes = Style::default().fg(Color::Black).bg(GREEN);
            let no = Style::default().fg(FG).bg(Color::Indexed(237));
            let lines = vec![
                Line::from("Everything will be overwritten. There is no recovery possible.")
                    .style(Style::default().fg(FG)),
                Line::from(""),
                Line::from(vec![
                    Span::styled(
                        format!("Confirm overwriting /dev/{disk}"),
                        Style::default().fg(BLUE),
                    ),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled(" Yes, format disk ", if w.yes_erase { yes } else { no }),
                    Span::raw("    "),
                    Span::styled(" No, change it ", if w.yes_erase { no } else { yes }),
                ]),
            ];
            f.render_widget(Paragraph::new(lines), centered(body, 56));
        }
        Screen::Installing | Screen::Done | Screen::Failed => {
            let mut lines: Vec<Line> = w
                .logs
                .iter()
                .map(|l| Line::from(Span::styled(l.clone(), Style::default().fg(FG))))
                .collect();
            if w.screen == Screen::Failed {
                lines.push(Line::from(Span::styled(
                    w.err.clone(),
                    Style::default().fg(Color::Red),
                )));
            }
            let n = lines.len();
            let h = body.height as usize;
            let skip = n.saturating_sub(h);
            f.render_widget(
                Paragraph::new(lines).scroll((skip as u16, 0)),
                centered(body, 72),
            );
        }
        _ => {}
    }

    if w.screen == Screen::Done {
        let secs = w.elapsed.as_secs();
        let done = Paragraph::new(vec![
            Line::from(""),
            Line::from(Span::styled(
                format!("Installed in {}m {}s", secs / 60, secs % 60),
                Style::default().fg(FG),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Remember to remove USB installer!",
                Style::default().fg(BLUE),
            )),
            Line::from(""),
            Line::from(Span::styled(
                " Reboot Now ",
                Style::default().fg(Color::Black).bg(GREEN),
            )),
        ])
        .alignment(Alignment::Center);
        f.render_widget(done, col[1]);
    }

    let hint = match w.screen {
        Screen::ConfirmErase => "←→  toggle • enter  submit • y  Yes, format disk • n  No, change it",
        Screen::Installing => "",
        Screen::Done => "enter  reboot • q  quit",
        Screen::Failed => "q  quit",
        s if input_screen(s) => "enter  submit",
        _ => "←↓↑→  navigate • enter  submit",
    };
    f.render_widget(
        Paragraph::new(hint)
            .alignment(Alignment::Center)
            .style(Style::default().fg(DIM)),
        col[3],
    );
}

fn centered(area: ratatui::layout::Rect, width: u16) -> ratatui::layout::Rect {
    let v = Layout::vertical([
        Constraint::Percentage(15),
        Constraint::Percentage(70),
        Constraint::Percentage(15),
    ])
    .split(area)[1];
    Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Length(width),
        Constraint::Fill(1),
    ])
    .split(v)[1]
}

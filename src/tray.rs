//! System tray icon, shown while "Close to tray" has hidden the window.
//!
//! The icon must be created on the UI thread (it owns a Win32 message window), so
//! the app creates and drops it from `logic`. Clicks arrive on that thread's message
//! loop; they are queued here and the UI is woken to handle them.

use std::sync::{Mutex, Once};

use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cmd {
    Show,
    Quit,
}

static PENDING: Mutex<Vec<Cmd>> = Mutex::new(Vec::new());

pub struct Tray {
    _icon: TrayIcon,
}

impl Tray {
    /// `wake` is called (from the UI thread's message loop) after each click.
    pub fn new(wake: impl Fn() + Send + Sync + Clone + 'static) -> anyhow::Result<Self> {
        // The handlers are process-wide and can only be set once.
        static HANDLERS: Once = Once::new();
        HANDLERS.call_once(|| {
            let w = wake.clone();
            TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
                if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = e {
                    push(Cmd::Show, &w);
                }
            }));
            MenuEvent::set_event_handler(Some(move |e: MenuEvent| match e.id.as_ref() {
                "show" => push(Cmd::Show, &wake),
                "quit" => push(Cmd::Quit, &wake),
                _ => {}
            }));
        });

        let menu = Menu::with_items(&[
            &MenuItem::with_id("show", "Show YT Music", true, None),
            &MenuItem::with_id("quit", "Quit", true, None),
        ])?;
        let png = image::load_from_memory(include_bytes!("../assets/icon/icon.png"))?
            .resize(32, 32, image::imageops::FilterType::Lanczos3)
            .into_rgba8();
        let (w, h) = png.dimensions();
        let icon = TrayIconBuilder::new()
            .with_icon(tray_icon::Icon::from_rgba(png.into_raw(), w, h)?)
            .with_tooltip("YT Music")
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .build()?;
        Ok(Self { _icon: icon })
    }
}

fn push(cmd: Cmd, wake: &impl Fn()) {
    PENDING.lock().unwrap().push(cmd);
    wake();
}

/// Clicks since the last call.
pub fn take() -> Vec<Cmd> {
    std::mem::take(&mut *PENDING.lock().unwrap())
}

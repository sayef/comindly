//! `comindly app`: the web UI in a native WebKit window, like Ollama's desktop app. The UI server
//! listens on a random local port with a token only this window knows.

use std::net::TcpListener;

use muda::accelerator::Accelerator;
use muda::{AboutMetadata, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem, Submenu};
use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::window::WindowBuilder;
use wry::{NewWindowResponse, WebViewBuilder};

enum Msg {
    Ready(String),
    Failed(String),
    Menu(MenuId),
    /// The page asked for a folder (Repositories: add or scan).
    PickFolder,
}

/// Open the window and serve the index at `root` into it; returns when the window closes.
pub fn run(root: String) -> anyhow::Result<()> {
    let event_loop = EventLoopBuilder::<Msg>::with_user_event().build();
    dock_icon();
    let proxy = event_loop.create_proxy();

    let key = |k: &str| k.parse::<Accelerator>().ok();
    let reload = MenuItem::new("Reload", true, key("CmdOrCtrl+R"));
    let actual = MenuItem::new("Actual Size", true, key("CmdOrCtrl+0"));
    let bigger = MenuItem::new("Zoom In", true, key("CmdOrCtrl+="));
    let smaller = MenuItem::new("Zoom Out", true, key("CmdOrCtrl+-"));
    let about = AboutMetadata {
        name: Some("comindly".into()),
        version: Some(env!("CARGO_PKG_VERSION").into()),
        ..Default::default()
    };
    let sep = PredefinedMenuItem::separator;
    let window_menu = Submenu::with_items(
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(None),
            &PredefinedMenuItem::maximize(None),
            &PredefinedMenuItem::fullscreen(None),
            &sep(),
            &PredefinedMenuItem::close_window(None),
        ],
    )?;
    let menu = Menu::with_items(&[
        &Submenu::with_items(
            "comindly",
            true,
            &[
                &PredefinedMenuItem::about(None, Some(about)),
                &sep(),
                &PredefinedMenuItem::hide(None),
                &PredefinedMenuItem::hide_others(None),
                &PredefinedMenuItem::show_all(None),
                &sep(),
                &PredefinedMenuItem::quit(None),
            ],
        )?,
        &Submenu::with_items(
            "Edit",
            true,
            &[
                &PredefinedMenuItem::undo(None),
                &PredefinedMenuItem::redo(None),
                &sep(),
                &PredefinedMenuItem::cut(None),
                &PredefinedMenuItem::copy(None),
                &PredefinedMenuItem::paste(None),
                &PredefinedMenuItem::select_all(None),
            ],
        )?,
        &Submenu::with_items("View", true, &[&reload, &sep(), &actual, &bigger, &smaller])?,
        &window_menu,
    ])?;
    menu.init_for_nsapp();
    window_menu.set_as_windows_menu_for_nsapp();
    let forward = std::sync::Mutex::new(proxy.clone());
    MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
        if let Ok(p) = forward.lock() {
            let _ = p.send_event(Msg::Menu(e.id));
        }
    }));

    let window = WindowBuilder::new()
        .with_title("comindly")
        .with_inner_size(LogicalSize::new(1320.0, 860.0))
        .with_min_inner_size(LogicalSize::new(880.0, 560.0))
        .build(&event_loop)?;
    let ask = std::sync::Mutex::new(proxy.clone());
    let webview = WebViewBuilder::new()
        .with_html(LOADING)
        .with_ipc_handler(move |req| {
            if req.body() == "pick-folder" {
                if let Ok(p) = ask.lock() {
                    let _ = p.send_event(Msg::PickFolder);
                }
            }
        })
        .with_navigation_handler(|url| {
            let local = ["http://127.0.0.1:", "about:", "data:"]
                .iter()
                .any(|p| url.starts_with(p));
            if !local {
                open_external(&url);
            }
            local
        })
        .with_new_window_req_handler(|url, _| {
            open_external(&url);
            NewWindowResponse::Deny
        })
        .build(&window)?;

    std::thread::spawn(move || {
        let started = (|| {
            let data = crate::tui::Data::load_or_empty(&root);
            let listener = TcpListener::bind("127.0.0.1:0")?;
            let port = listener.local_addr()?.port();
            let token = crate::web::new_token();
            let url = format!("http://127.0.0.1:{port}/?token={token}");
            let _ = proxy.send_event(Msg::Ready(url));
            crate::web::serve(data, listener, crate::web::Access::Local(token))
        })();
        if let Err(e) = started {
            let _ = proxy.send_event(Msg::Failed(format!("{e:#}")));
        }
    });

    let mut zoom = 1.0_f64;
    event_loop.run(move |event, _, flow| {
        *flow = ControlFlow::Wait;
        let _ = (&window, &menu);
        match event {
            Event::UserEvent(Msg::Ready(url)) => {
                let _ = webview.load_url(&url);
            }
            Event::UserEvent(Msg::PickFolder) => {
                let picked = rfd::FileDialog::new()
                    .pick_folder()
                    .map(|p| p.to_string_lossy().into_owned());
                let arg = serde_json::to_string(&picked).unwrap_or_else(|_| "null".into());
                let _ = webview.evaluate_script(&format!(
                    "window.__comindlyPicked && window.__comindlyPicked({arg})"
                ));
            }
            Event::UserEvent(Msg::Failed(e)) => {
                let _ = webview.load_html(&failed(&e, &root_hint()));
            }
            Event::UserEvent(Msg::Menu(id)) => {
                if id == *reload.id() {
                    let _ = webview.reload();
                } else {
                    zoom = if id == *bigger.id() {
                        (zoom + 0.1).min(2.0)
                    } else if id == *smaller.id() {
                        (zoom - 0.1).max(0.5)
                    } else if id == *actual.id() {
                        1.0
                    } else {
                        return;
                    };
                    let _ = webview.zoom(zoom);
                }
            }
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => *flow = ControlFlow::Exit,
            _ => {}
        }
    })
}

fn open_external(url: &str) {
    let _ = std::process::Command::new("open").arg(url).spawn();
}

fn root_hint() -> String {
    crate::config::store_path().display().to_string()
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn failed(error: &str, config: &str) -> String {
    format!(
        r#"<!doctype html><html><head><meta charset="utf-8"><style>{STYLE}</style></head><body><main>
<h1>comindly could not open the index</h1><pre>{}</pre>
<p>Set <code>index_dir</code> in <code>{}</code>, or start the app from a terminal with
<code>comindly app --index-dir …</code>. An index in a bucket needs the platform's credentials in the
environment the app starts from.</p></main></body></html>"#,
        escape(error),
        escape(config)
    )
}

const STYLE: &str = "body{margin:0;height:100vh;display:grid;place-items:center;font:14px -apple-system,system-ui,sans-serif;\
color:#1f2733;background:#f7f8fa}main{max-width:36rem;padding:0 1.5rem}h1{font-size:18px}\
pre{white-space:pre-wrap;background:#eef1f5;padding:.75rem;border-radius:8px;font-size:12px}\
code{background:#eef1f5;padding:.1rem .3rem;border-radius:4px}p{color:#5b6574;line-height:1.5}\
.dot{width:10px;height:10px;border-radius:50%;background:#3fa45b;margin:0 auto 14px;animation:p 1s ease-in-out infinite alternate}\
@keyframes p{from{opacity:.25}to{opacity:1}}\
@media (prefers-color-scheme:dark){body{color:#e6e9ef;background:#151a22}pre,code{background:#232a35}p{color:#9aa4b2}}";

const LOADING: &str = concat!(
    r#"<!doctype html><html><head><meta charset="utf-8"><style>"#,
    "body{margin:0;height:100vh;display:grid;place-items:center;font:14px -apple-system,system-ui,sans-serif;\
color:#5b6574;background:#f7f8fa}.dot{width:10px;height:10px;border-radius:50%;background:#3fa45b;\
margin:0 auto 14px;animation:p 1s ease-in-out infinite alternate}@keyframes p{from{opacity:.25}to{opacity:1}}\
@media (prefers-color-scheme:dark){body{color:#9aa4b2;background:#151a22}}",
    r#"</style></head><body><div><div class="dot"></div>Loading the index…</div></body></html>"#
);

/// Show comindly's icon in the Dock and the app switcher; a plain binary has no bundle icon.
fn dock_icon() {
    use objc2::{AllocAnyThread, MainThreadMarker};
    use objc2_app_kit::{NSApplication, NSImage};
    use objc2_foundation::NSData;
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let data = NSData::with_bytes(include_bytes!("app-icon.png"));
    if let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) {
        unsafe { NSApplication::sharedApplication(mtm).setApplicationIconImage(Some(&image)) };
    }
}

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod player;

use player::{
    get_pos, get_thumbnail_base64, get_title, next, pause, play, player_loop, previous, quit,
    set_position_command, volume_down, volume_up, PlayerCommand,
};
use tokio::runtime::Builder;
use tokio::sync::mpsc;
use tokio::task::LocalSet;

use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Emitter, Manager,
};

fn main() {
    let runtime = Builder::new_current_thread().enable_all().build().unwrap();

    let (cmd_tx, cmd_rx) = mpsc::channel::<PlayerCommand>(32);

    tauri::Builder::default()
        .plugin(tauri_plugin_window_state::Builder::new().build())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    use tauri_plugin_global_shortcut::{Code, Modifiers, Shortcut, ShortcutState};

                    let play_pause_shortcut =
                        Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyP);
                    let toggle_window_shortcut =
                        Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyP);

                    if shortcut == &play_pause_shortcut {
                        match event.state() {
                            ShortcutState::Released => app
                                .emit("tray-play-pause", {})
                                .expect("global shortcut error"),
                            _ => (),
                        }
                    }

                    if shortcut == &toggle_window_shortcut {
                        match event.state() {
                            ShortcutState::Released => {
                                app.emit("show-hide", {}).expect("global shortcut error")
                            }
                            _ => (),
                        }
                    }
                })
                .build(),
        )
        .manage(cmd_tx.clone())
        .setup(move |app| {
            let app_handle: AppHandle = app.handle().clone();
            let rx_for_player = cmd_rx;

            if let Some(window) = app.get_webview_window("main") {
                let _ = window.hide();
            }

            std::thread::spawn(move || {
                let local_set = LocalSet::new();
                local_set.spawn_local(async move {
                    if let Err(e) = player_loop(rx_for_player, app_handle).await {
                        eprintln!("Player loop error: {:?}", e);
                    }
                });
                runtime.block_on(local_set);
            });

            // ---- TRAY ICON SETUP ----
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let toggle_visibility_i = MenuItem::with_id(
                app,
                "toggle_visibility",
                "Show/Hide Window",
                true,
                None::<&str>,
            )?;
            let play_pause =
                MenuItem::with_id(app, "play_pause", "Play/Pause", true, None::<&str>)?;
            let next = MenuItem::with_id(app, "next", "Next Song", true, None::<&str>)?;
            let previous = MenuItem::with_id(app, "previous", "Previous Song", true, None::<&str>)?;

            let tray_menu = Menu::with_items(
                app,
                &[&toggle_visibility_i, &play_pause, &next, &previous, &quit_i],
            )?;

            TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .menu(&tray_menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "toggle_visibility" => {
                        let window = app
                            .get_webview_window("main")
                            .expect("no se logro obtener la ventana");
                        if window
                            .is_visible()
                            .expect("no me logre poner invisible lol")
                        {
                            if window.is_minimized().expect("noooooo") {
                                let _ = window.hide();
                                let _ = window.unminimize();
                            } else {
                                let _ = window.hide();
                            }
                        } else {
                            let _ = window.set_always_on_top(true);
                            let _ = window.show();
                            let _ = window.unminimize();
                            // let _ = window.set_always_on_top(false);
                        }
                    }
                    "quit" => app.exit(0),
                    "play_pause" => {
                        let _ = app.emit("tray-play-pause", {});
                    }
                    "next" => {
                        let _ = app.emit("tray-next", {});
                    }
                    "previous" => {
                        let _ = app.emit("tray-previous", {});
                    }
                    _ => {}
                })
                .build(app)?;

            // ---- REGISTER SHORTCUTS ----
            #[cfg(desktop)]
            {
                use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut};

                let play_pause_shortcut =
                    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyP);
                let toggle_window_shortcut =
                    Shortcut::new(Some(Modifiers::CONTROL | Modifiers::SHIFT), Code::KeyP);

                app.global_shortcut().register(play_pause_shortcut)?;
                app.global_shortcut().register(toggle_window_shortcut)?;
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            play,
            pause,
            next,
            previous,
            get_title,
            get_pos,
            get_thumbnail_base64,
            set_position_command,
            quit,
            volume_up,
            volume_down
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri app");
}

// Commandes : show | hide | center | reveal | conceal | pass on|off | mon <i> | kb none|demand|exclusive | list | quit
use gtk::prelude::*;
use gtk_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use std::io::BufRead;
use std::time::Instant;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

fn main() {
    let unrealize = std::env::args().all(|a| a != "--no-unrealize");
    tauri::Builder::default()
        .setup(move |app| {
            let win = WebviewWindowBuilder::new(app, "osd", WebviewUrl::App("index.html".into()))
                .title("spike-layer")
                .visible(false)
                .decorations(false)
                .transparent(true)
                .inner_size(300.0, 100.0)
                .on_document_title_changed(|_, t| println!("js {t}"))
                .build()?;
            let gtk = win.gtk_window()?;
            println!("realized before init: {}", gtk.is_realized());
            gtk.hide();
            if unrealize {
                gtk.unrealize();
            }
            println!("realized after step: {}", gtk.is_realized());
            gtk.init_layer_shell();
            gtk.set_namespace("fader-spike");
            gtk.set_layer(Layer::Overlay);
            gtk.set_keyboard_mode(KeyboardMode::None);
            gtk.set_anchor(Edge::Top, true);
            gtk.set_size_request(300, 100);
            println!("is_layer_window: {}", gtk.is_layer_window());

            let t0 = Instant::now();
            gtk.connect_show(move |_| println!("gtk-show {:?}", t0.elapsed()));
            gtk.connect_map_event(move |_, _| {
                println!("map {:?}", t0.elapsed());
                gtk::glib::Propagation::Proceed
            });
            gtk.connect_focus_in_event(move |_, _| {
                println!("focus-in {:?}", t0.elapsed());
                gtk::glib::Propagation::Proceed
            });
            gtk.connect_focus_out_event(move |_, _| {
                println!("focus-out {:?}", t0.elapsed());
                gtk::glib::Propagation::Proceed
            });
            gtk.connect_key_press_event(move |_, e| {
                println!("key {:?} {:?}", e.keyval(), t0.elapsed());
                gtk::glib::Propagation::Proceed
            });

            let handle = app.handle().clone();
            std::thread::spawn(move || {
                for line in std::io::stdin().lock().lines().map_while(Result::ok) {
                    let Some(win) = handle.get_webview_window("osd") else {
                        return;
                    };
                    let h = handle.clone();
                    let w = win.clone();
                    let _ = win.run_on_main_thread(move || {
                        let Ok(g) = w.gtk_window() else { return };
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        match parts.as_slice() {
                            ["show"] => g.show_all(),
                            ["center"] => g.set_anchor(Edge::Top, false),
                            ["reveal"] => {
                                let _ = w.eval("document.body.style.visibility='visible'");
                            }
                            ["conceal"] => {
                                let _ = w.eval("document.body.style.visibility='hidden'");
                            }
                            ["pass", "on"] => {
                                g.input_shape_combine_region(Some(&gtk::cairo::Region::create()))
                            }
                            ["pass", "off"] => g.input_shape_combine_region(None),
                            ["pass", "gdk"] => {
                                if let Some(gw) = g.window() {
                                    gw.input_shape_combine_region(
                                        &gtk::cairo::Region::create_rectangle(
                                            &gtk::cairo::RectangleInt::new(0, 0, 300, 100),
                                        ),
                                        0,
                                        0,
                                    );
                                }
                            }
                            ["pass", "full"] => g.input_shape_combine_region(Some(
                                &gtk::cairo::Region::create_rectangle(
                                    &gtk::cairo::RectangleInt::new(0, 0, 300, 100),
                                ),
                            )),
                            ["hide"] => g.hide(),
                            ["list"] | ["mon", _] => {
                                let d = gtk::gdk::Display::default().expect("display");
                                if let ["mon", i] = parts.as_slice() {
                                    let m = d.monitor(i.parse().unwrap_or(0)).expect("monitor");
                                    g.set_monitor(&m);
                                } else {
                                    for i in 0..d.n_monitors() {
                                        let m = d.monitor(i).expect("monitor");
                                        let r = m.geometry();
                                        println!(
                                            "gdk {i}: {:?} {:?} at {},{} {}x{}",
                                            m.manufacturer(),
                                            m.model(),
                                            r.x(),
                                            r.y(),
                                            r.width(),
                                            r.height()
                                        );
                                    }
                                }
                            }
                            ["kb", k] => g.set_keyboard_mode(match *k {
                                "exclusive" => KeyboardMode::Exclusive,
                                "demand" => KeyboardMode::OnDemand,
                                _ => KeyboardMode::None,
                            }),
                            ["quit"] => h.exit(0),
                            _ => println!("? {line}"),
                        }
                        println!("done {line}");
                    });
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("run");
}

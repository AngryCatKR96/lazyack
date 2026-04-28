use std::collections::HashMap;
use std::sync::Arc;
use std::thread;

use core_foundation::runloop::CFRunLoop;
use global_hotkey::{
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
    hotkey::{Code, HotKey, Modifiers},
};

fn main() {
    let manager = GlobalHotKeyManager::new().expect("create hotkey manager");

    let bindings = [
        ("1", HotKey::new(Some(Modifiers::META | Modifiers::ALT), Code::Digit1)),
        ("2", HotKey::new(Some(Modifiers::META | Modifiers::ALT), Code::Digit2)),
        ("3", HotKey::new(Some(Modifiers::META | Modifiers::ALT), Code::Digit3)),
    ];

    let mut name_by_id: HashMap<u32, &'static str> = HashMap::new();
    for (name, hk) in &bindings {
        manager.register(*hk).expect("register hotkey");
        name_by_id.insert(hk.id(), name);
    }
    let name_by_id = Arc::new(name_by_id);

    println!("lazyack PoC ready.");
    println!("Press Cmd+Opt+1 / Cmd+Opt+2 / Cmd+Opt+3 anywhere on the system.");
    println!("Ctrl+C to exit.");
    println!();
    println!(
        "First run: macOS will ask for Accessibility / Input Monitoring permission for your terminal."
    );
    println!("Grant it in System Settings and re-run.");
    println!();

    let names = name_by_id.clone();
    thread::spawn(move || {
        let receiver = GlobalHotKeyEvent::receiver();
        while let Ok(event) = receiver.recv() {
            if event.state == HotKeyState::Pressed {
                let name = names.get(&event.id).copied().unwrap_or("?");
                println!("[hotkey] Cmd+Opt+{name} pressed");
            }
        }
    });

    // Carbon HotKey events dispatch through the main-thread run loop.
    CFRunLoop::run_current();

    drop(manager);
}

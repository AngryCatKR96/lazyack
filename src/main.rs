use std::collections::HashMap;
use std::time::{Duration, Instant};

use global_hotkey::{
    GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState,
    hotkey::{Code, HotKey, Modifiers},
};
use tao::event_loop::{ControlFlow, EventLoopBuilder};

fn main() {
    let event_loop = EventLoopBuilder::new().build();

    let manager = GlobalHotKeyManager::new().expect("create hotkey manager");

    let mods = Modifiers::CONTROL | Modifiers::ALT | Modifiers::SHIFT;
    let bindings = [
        ("1", HotKey::new(Some(mods), Code::Digit1)),
        ("2", HotKey::new(Some(mods), Code::Digit2)),
        ("3", HotKey::new(Some(mods), Code::Digit3)),
    ];

    let mut name_by_id: HashMap<u32, &'static str> = HashMap::new();
    for (name, hk) in &bindings {
        manager.register(*hk).expect("register hotkey");
        name_by_id.insert(hk.id(), name);
    }

    println!("lazyack PoC ready. Press Ctrl+Opt+Shift+1 / 2 / 3 anywhere.");
    println!("Ctrl+C to exit.");

    let receiver = GlobalHotKeyEvent::receiver();

    event_loop.run(move |_event, _, control_flow| {
        *control_flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(300));

        while let Ok(event) = receiver.try_recv() {
            if event.state == HotKeyState::Pressed {
                let name = name_by_id.get(&event.id).copied().unwrap_or("?");
                println!("[hotkey] Ctrl+Opt+Shift+{name}");
            }
        }
    });
}

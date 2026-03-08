use constcat::concat;
use streamdeck_lib::{incoming::*, prelude::*};
use tracing::{debug, info};

use crate::PLUGIN_ID;

/// Example action — replace with your own.
///
/// Demonstrates:
/// - Action registration via `ActionStatic`
/// - Settings handling via `did_receive_settings` / `will_appear`
/// - Key press handling
/// - Topic subscriptions via `on_notify`
#[derive(Default)]
pub struct ExampleAction {
    // Per-instance state goes here.
    // Each Stream Deck key that uses this action gets its own instance.
}

impl ActionStatic for ExampleAction {
    const ID: &'static str = concat!(PLUGIN_ID, ".example");
}

impl Action for ExampleAction {
    fn id(&self) -> &str {
        Self::ID
    }

    /// Subscribe to topics from `topics.rs`.
    /// The runtime will call `on_notify` for each matching publish.
    fn topics(&self) -> &'static [&'static str] {
        &[
            // Example: crate::topics::MY_TOPIC.name,
        ]
    }

    fn will_appear(&mut self, cx: &Context, ev: &WillAppear) {
        debug!("ExampleAction will_appear: {}", ev.context);
        // Read initial settings and configure the key.
        // ev.settings contains the per-key settings Map.
    }

    fn did_receive_settings(&mut self, _cx: &Context, ev: &DidReceiveSettings) {
        debug!("ExampleAction settings: {:?}", ev.settings);
        // Called when the Property Inspector changes a setting.
    }

    fn key_down(&mut self, cx: &Context, ev: &KeyDown) {
        info!("ExampleAction key_down: {}", ev.context);
        cx.sd().show_ok(ev.context);
    }

    fn key_up(&mut self, _cx: &Context, ev: &KeyUp) {
        debug!("ExampleAction key_up: {}", ev.context);
    }

    fn on_notify(&mut self, _cx: &Context, _ctx_id: &str, _event: &ErasedTopic) {
        // Handle topic events here.
        // Example:
        //   if let Some(payload) = event.downcast(crate::topics::MY_TOPIC) {
        //       // react to payload
        //   }
    }
}

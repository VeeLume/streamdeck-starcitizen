use streamdeck_lib::prelude::*;

// ── Topic Definitions ─────────────────────────────────────────────────────────
//
// Topics are typed pub/sub channels used to decouple actions, adapters, and state.
//
// Define a topic:
//   pub const MY_TOPIC: TopicId<MyPayload> = TopicId::new("myplugin.my-topic");
//
// Publish (from action, adapter, or hook):
//   cx.bus().publish_t(MY_TOPIC, payload);
//
// Subscribe (in an action):
//   fn topics(&self) -> &'static [&'static str] { &[MY_TOPIC.name] }
//   fn on_notify(&mut self, cx: &Context, ctx_id: &str, event: &ErasedTopic) {
//       if let Some(payload) = event.downcast(MY_TOPIC) { /* ... */ }
//   }
//
// Subscribe (in an adapter):
//   fn topics(&self) -> &'static [&'static str] { &[MY_TOPIC.name] }
//   // received via the inbox channel in start()

// Example:
// pub const EXAMPLE_EVENT: TopicId<String> = TopicId::new("starcitizen.example-event");

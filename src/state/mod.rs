// Shared state stores for this plugin.
//
// Pattern: Create a struct per logical state concern, wrap in Arc<T>,
// register via Plugin::add_extension(), retrieve via cx.try_ext::<T>().
//
// Recommended backing types:
//   - arc_swap::ArcSwap<T>  — lock-free reads, rare writes
//   - parking_lot::RwLock<T> — frequent reads, occasional writes
//   - dashmap::DashMap<K,V>  — concurrent map access
//
// Example:
//
// ```rust
// use arc_swap::ArcSwap;
// use std::sync::Arc;
//
// pub struct MyStore {
//     inner: Arc<ArcSwap<MyData>>,
// }
//
// impl MyStore {
//     pub fn new() -> Self {
//         Self { inner: Arc::new(ArcSwap::from_pointee(MyData::default())) }
//     }
//     pub fn snapshot(&self) -> Arc<MyData> { self.inner.load_full() }
//     pub fn replace(&self, data: MyData) { self.inner.store(Arc::new(data)); }
// }
// ```
//
// Register in main.rs:
//   .add_extension(Arc::new(MyStore::new()))
//
// Use in actions/adapters:
//   let store = cx.try_ext::<MyStore>().expect("MyStore not registered");
//   let data = store.snapshot();

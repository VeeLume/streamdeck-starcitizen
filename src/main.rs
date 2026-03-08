mod actions;
mod state;
mod topics;

use streamdeck_lib::prelude::*;
use tracing::info;

use actions::example::ExampleAction;

pub const PLUGIN_ID: &str = "icu.veelume.starcitizen";

fn main() -> anyhow::Result<()> {
    let _guard = init(PLUGIN_ID);

    info!("Starting starcitizen Stream Deck plugin");

    let plugin = Plugin::new().add_action(ActionFactory::default_of::<ExampleAction>());

    run_plugin(plugin)
}

use tracing::{Level, level_filters::LevelFilter};
use tracing_subscriber::{
    Layer, Registry, filter::Targets, layer::SubscriberExt, util::SubscriberInitExt,
};

pub fn init() {
    tracing_subscriber::registry().with(stderr_layer()).init();
}

fn stderr_layer() -> impl Layer<Registry> {
    let targets = Targets::new()
        .with_default(LevelFilter::INFO)
        .with_target("egui_wgpu", Level::WARN)
        .with_target("wgpu_hal", Level::WARN)
        .with_target("sctk_adwaita", Level::ERROR);

    tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_filter(targets)
}

use clap::Parser;
use tracing::{Level, level_filters::LevelFilter};
use tracing_subscriber::{
    Layer, Registry, filter::Targets, layer::SubscriberExt, util::SubscriberInitExt,
};

pub fn init(options: &LogOptions) {
    tracing_subscriber::registry()
        .with(stderr_layer(options))
        .init();
}

/// Command-line options for configuring logging.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Parser)]
pub struct LogOptions {
    /// The logging level to use.
    #[clap(long, global = true, default_value = "info")]
    pub log_level: Level,

    /// Filters specific logging targets.
    ///
    /// If no level is specified, the target will be disabled.
    #[clap(long, value_name = "TARGET[=LEVEL]", global = true)]
    pub filter_log_target: Vec<TargetFilter>,
}

/// Represents a filter for a specific logging target.
///
/// Syntax: `<target>[=<level>]`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TargetFilter {
    /// The name of the logging target.
    pub target: String,

    /// The logging level for the target.
    ///
    /// If [`None`], the target will be disabled.
    pub level: Option<Level>,
}

impl std::str::FromStr for TargetFilter {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.trim();
        if s.is_empty() {
            return Err("Target filter cannot be empty".into());
        }
        let mut parts = s.splitn(2, '=');
        let target = parts.next().unwrap().trim().to_string();
        let level = match parts.next() {
            Some(level_str) => Some(level_str.parse::<Level>().map_err(|e| e.to_string())?),
            None => None,
        };
        Ok(TargetFilter { target, level })
    }
}

fn stderr_layer(options: &LogOptions) -> impl Layer<Registry> {
    let mut targets = Targets::new()
        .with_default(LevelFilter::from(options.log_level))
        .with_target("egui_wgpu", Level::WARN)
        .with_target("wgpu_hal", Level::WARN)
        .with_target("sctk_adwaita", Level::ERROR);

    for filter in &options.filter_log_target {
        if let Some(level) = filter.level {
            targets = targets.with_target(&filter.target, level);
        } else {
            targets = targets.with_target(&filter.target, LevelFilter::OFF);
        }
    }

    tracing_subscriber::fmt::layer()
        .with_writer(std::io::stderr)
        .with_filter(targets)
}

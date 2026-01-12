//! Sashiki - A lightweight cockpit for AI agents
//!
//! Built with iced (Elm-inspired GUI framework)

mod app;
mod config;
mod diff;
mod git;
mod session;
mod terminal;
mod theme;

use app::Sashiki;
use iced::Size;

fn main() -> iced::Result {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    tracing::info!("Starting Sashiki");

    iced::application(Sashiki::new, Sashiki::update, Sashiki::view)
        .title("Sashiki")
        .theme(Sashiki::theme)
        .subscription(Sashiki::subscription)
        .window_size(Size::new(1280.0, 800.0))
        .centered()
        .run()
}

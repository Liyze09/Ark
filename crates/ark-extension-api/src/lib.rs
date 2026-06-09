wit_bindgen::generate!({
    path: "../ark-runtime/wit",
    world: "core",
    pub_export_macro: true,
    enable_method_chaining: true,
    generate_unused_types: false,
    async: false,
});

mod vk {
    wit_bindgen::generate!({
        path: "../ark-vk-binding/wit",
        world: "vulkan",
        pub_export_macro: true,
        enable_method_chaining: true,
        generate_unused_types: false,
        default_bindings_module: "vulkan",
        async: false,
    });
}

pub mod reg {
    wit_bindgen::generate!({
        path: "../ark-runtime/wit",
        world: "entry",
        pub_export_macro: true,
        enable_method_chaining: true,
        generate_unused_types: false,
        async: false,
    });
}

use log::Log;

use crate::core::logging::*;

struct HostLogger;

impl Log for HostLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        is_enabled(match metadata.level() {
            log::Level::Error => Level::Error,
            log::Level::Warn => Level::Warn,
            log::Level::Info => Level::Info,
            log::Level::Debug => Level::Debug,
            log::Level::Trace => Level::Trace,
        })
    }

    fn log(&self, record: &log::Record) {
        match record.level() {
            log::Level::Error => error(&record.args().to_string()),
            log::Level::Warn => warn(&record.args().to_string()),
            log::Level::Info => info(&record.args().to_string()),
            log::Level::Debug => debug(&record.args().to_string()),
            log::Level::Trace => trace(&record.args().to_string()),
        }
    }

    fn flush(&self) {
        
    }
}

pub use crate::vk::ark::gpu as vulkan;
pub use crate::ark::core as core;
pub use crate::reg::exports::ark::core::entrance::Guest as ExtensionInitializer;
#[macro_export]
macro_rules! initialize {
    ($ident:tt) => {
        ark_extension_api::reg::export!($ident with_types_in ark_extension_api::reg);
    };
}

pub fn init_logger() {
    log::set_logger(Box::leak(Box::new(HostLogger {}))).expect("Failed to set logger");
    log::set_max_level(log::LevelFilter::Trace);
}

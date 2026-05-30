pub mod entry;
use wasmtime::
    component::{Access, Linker, bindgen}
;

use crate::extension::{binding::ark::core::logging::{Level}, wasm::ExtensionContext};

bindgen!({
    world: "core",
    anyhow: true,
    imports: {
        "register": store,
    },
});

pub(crate) fn add_to_linker(linker: &mut Linker<ExtensionContext>) -> Result<(), wasmtime::Error> {
    ark::core::logging::add_to_linker::<ExtensionContext, ExtensionContext>(linker, |data:&mut ExtensionContext| data)?;
    Core::add_to_linker::<ExtensionContext, ExtensionContext>(linker, |data:&mut ExtensionContext| data)?;
    Ok(())
}

impl ark::core::logging::Host for ExtensionContext {
    fn trace(&mut self, message: String) {
        log::trace!("[ark-ext-{}] {}", self.package.manifest.id, message);
    }

    fn debug(&mut self, message: String) {
        log::debug!("[ark-ext-{}] {}", self.package.manifest.id, message)
    }

    fn info(&mut self, message: String) {
        log::info!("[ark-ext-{}] {}", self.package.manifest.id, message)
    }

    fn warn(&mut self, message: String) {
        log::warn!("[ark-ext-{}] {}", self.package.manifest.id, message)
    }

    fn error(&mut self, message: String) {
        log::error!("[ark-ext-{}] {}", self.package.manifest.id, message)
    }

    fn log(&mut self, level: Level, message: String) {
        match level {
            Level::Trace => self.trace(message),
            Level::Debug => self.debug(message),
            Level::Info => self.info(message),
            Level::Warn => self.warn(message),
            Level::Error => self.error(message),
        }
    }

    fn is_enabled(&mut self,level: Level,) -> bool {
        match level {
            Level::Trace => log::log_enabled!(log::Level::Trace),
            Level::Debug => log::log_enabled!(log::Level::Debug),
            Level::Info => log::log_enabled!(log::Level::Info),
            Level::Warn => log::log_enabled!(log::Level::Warn),
            Level::Error => log::log_enabled!(log::Level::Error),
        }
    }
}

impl CoreImportsWithStore for ExtensionContext {
    fn register<T>(
        mut host: Access<'_, T, Self>,
        trigger: String,
    ) -> Result<(), String> {
        let data = host.get();
        let id = data.package.manifest.id.clone();
        let mut registry = data.public_registry.lock();
        if let Some(value) = registry.get_mut(&trigger) {
            value.push(id);
        } else {
            registry.insert(trigger, vec![id]);
        }
        Ok(())
    }
}

impl CoreImports for ExtensionContext {
    fn check_vulkan_feature(&mut self, feature: String) -> bool {
        self.enabled_vulkan_features
            .lock()
            .contains(&feature)
    }

    fn check_vulkan_extension(&mut self, extension: String) -> bool {
        self.enabled_vulkan_extensions
            .lock()
            .contains(&extension)
    }
}

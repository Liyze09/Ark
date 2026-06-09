use std::{
    collections::{HashMap, HashSet},
    net::{IpAddr, SocketAddr, ToSocketAddrs},
    path::{self, PathBuf},
    sync::Arc,
};

use log::warn;
use parking_lot::Mutex;
use thiserror::Error;
use wasmtime::{
    Cache, CacheConfig, Config, Engine, Store, Trap, WasmCoreDump,
    component::{Component, HasData, Linker},
};
use wasmtime_wasi::{
    DirPerms, FilePerms, ResourceTable, WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView,
};

use crate::{
    extension::{
        binding::{self, entry::Entry},
        package::{ExtensionPackage, parse_package},
    },
    vulkan::VkBackend,
};

#[derive(Debug, Error)]
pub enum ExtensionError {
    /// WASM trap from inside the extension — inspect with
    /// `source.downcast_ref::<wasmtime::Trap>()` to obtain the
    /// [`wasmtime::Trap`] (trap code, backtrace, etc.).
    #[error("WASM trap in extension '{id}': {source}")]
    WasmTrap {
        id: String,
        #[source]
        source: wasmtime::Error,
    },
    /// Host-side runtime error (I/O, parsing, locking, extension-not-found, etc.)
    #[error("{0}")]
    Runtime(#[from] anyhow::Error),
    /// Multiple extensions failed during batch initialization.
    #[error("{count} error(s) occurred: {errors:?}")]
    Multi {
        count: usize,
        errors: Vec<ExtensionError>,
    },
}

impl ExtensionError {
    /// Wrap a [`wasmtime::Error`], distinguishing WASM traps from host errors.
    fn from_wasmtime(err: wasmtime::Error, ext_id: &str) -> Self {
        if err.downcast_ref::<Trap>().is_some() {
            Self::WasmTrap {
                id: ext_id.to_string(),
                source: err,
            }
        } else {
            Self::Runtime(err.into())
        }
    }

    /// Extract the [`WasmCoreDump`] from a trap error, if one was captured.
    ///
    /// Core dumps are only produced when
    /// [`Config::coredump_on_trap`](wasmtime::Config::coredump_on_trap) is
    /// enabled and a WASM trap actually occurs.  The dump lives as context
    /// on the [`wasmtime::Error`] — use `source.downcast_ref::<WasmCoreDump>()`
    /// rather than looking for it on the [`Trap`][crate::Trap] directly.
    pub fn coredump(&self) -> Option<&WasmCoreDump> {
        match self {
            Self::WasmTrap { source, .. } => source.downcast_ref::<WasmCoreDump>(),
            _ => None,
        }
    }
}

impl From<std::io::Error> for ExtensionError {
    fn from(e: std::io::Error) -> Self {
        Self::Runtime(e.into())
    }
}

pub struct WasmRuntime {
    pub engine: Engine,
    pub loaded_extensions: LoadedExtensions,
    pub linker: Linker<ExtensionContext>,
    pub registry: Registry,
    pub extension_folder: String,
    pub vulkan: VkBackend,
    pub vk_ctx: ark_vk_binding::VkContextOwned,
    pub enabled_vulkan_features: Arc<Mutex<HashSet<String>>>,
    pub enabled_vulkan_extensions: Arc<Mutex<HashSet<String>>>,
}

type LoadedExtensions = Arc<Mutex<HashMap<String, (Store<ExtensionContext>, Entry)>>>;

type Registry = Arc<Mutex<HashMap<String, Vec<String>>>>;

static CACHE_PATH: &str = "./cache/ark/";

pub struct ExtensionContext {
    pub package: ExtensionPackage,
    pub wasi_ctx: WasiCtx,
    pub table: ResourceTable,
    pub public_registry: Registry,
    pub enabled_vulkan_features: Arc<Mutex<HashSet<String>>>,
    pub enabled_vulkan_extensions: Arc<Mutex<HashSet<String>>>,
    pub vk_ctx: ark_vk_binding::VkContextOwned,
}

unsafe impl Send for ExtensionContext {}

impl ark_vk_binding::VkView for ExtensionContext {
    fn ctx(&mut self) -> ark_vk_binding::VkContextView<'_> {
        ark_vk_binding::VkContextView {
            owned: &self.vk_ctx,
            table: &mut self.table,
            files: &self.package.files,
        }
    }
}

impl HasData for ExtensionContext {
    type Data<'a> = &'a mut ExtensionContext;
}

impl WasmRuntime {
    pub fn new(extension_folder: String, vulkan: VkBackend) -> anyhow::Result<Self> {
        let mut config = Config::new();

        let cache_file = path::absolute(CACHE_PATH)?;
        let mut cache_config = CacheConfig::new();
        cache_config.with_directory(cache_file);
        let cache = Cache::new(cache_config)?;
        config.cache(Some(cache));
        config.coredump_on_trap(true);

        let vk_ctx = unsafe { vulkan.to_vk_context() };
        let engine = Engine::new(&config)?;
        let mut linker = Linker::<ExtensionContext>::new(&engine);
        wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;
        binding::add_to_linker(&mut linker)?;
        ark_vk_binding::add_to_linker(&mut linker)?;
        Ok(Self {
            linker,
            engine,
            loaded_extensions: Arc::new(Mutex::new(HashMap::new())),
            registry: Arc::new(Mutex::new(HashMap::new())),
            extension_folder,
            vulkan,
            vk_ctx,
            enabled_vulkan_features: Arc::new(Mutex::new(HashSet::new())),
            enabled_vulkan_extensions: Arc::new(Mutex::new(HashSet::new())),
        })
    }

    pub fn load_extension(&self, file_name: &str, args: LaunchArgs) -> Result<(), ExtensionError> {
        let path = PathBuf::new().join(&self.extension_folder).join(file_name);
        let bytes = std::fs::read(path)?;
        self.load_extension_by_bytes(&bytes, args)
    }

    pub fn load_extension_by_bytes(
        &self,
        bytes: &[u8],
        args: LaunchArgs,
    ) -> Result<(), ExtensionError> {
        let package = parse_package(bytes)?;
        let wasm_bytes = package
            .files
            .get(package.manifest.entrypoint.as_str())
            .ok_or_else(|| anyhow::anyhow!("Failed to find entrypoint wasm file in package"))?;
        let wasm_component = Component::from_binary(&self.engine, wasm_bytes)
            .map_err(|e| ExtensionError::from_wasmtime(e, &package.manifest.id))?;

        let mut wasi_builder = WasiCtxBuilder::new();
        wasi_builder.allow_blocking_current_thread(true);
        let mut network_addrs: Vec<(IpAddr, Option<u16>)> = Vec::new();

        for feature_str in &args.enabled_wasi_features {
            match parse_wasi_feature(feature_str)? {
                WasiFeature::Filesystem {
                    host_path,
                    guest_path,
                } => {
                    wasi_builder
                        .preopened_dir(&host_path, &guest_path, DirPerms::all(), FilePerms::all())
                        .map_err(|e| ExtensionError::Runtime(e.into()))?;
                }
                WasiFeature::Network { addrs } => network_addrs.extend(addrs),
            }
        }

        if !network_addrs.is_empty() {
            wasi_builder.allow_ip_name_lookup(true);
            let allowed = Arc::new(network_addrs);
            wasi_builder.socket_addr_check(move |addr, _use| {
                let allowed = Arc::clone(&allowed);
                Box::pin(async move {
                    allowed
                        .iter()
                        .any(|(ip, port)| *ip == addr.ip() && port.is_none_or(|p| p == addr.port()))
                })
            });
        }

        let vk_ctx = unsafe { self.vulkan.to_vk_context() };
        let mut store = Store::new(
            &self.engine,
            ExtensionContext {
                package,
                wasi_ctx: wasi_builder.build(),
                table: ResourceTable::new(),
                public_registry: self.registry.clone(),
                enabled_vulkan_features: self.enabled_vulkan_features.clone(),
                enabled_vulkan_extensions: self.enabled_vulkan_extensions.clone(),
                vk_ctx,
            },
        );
        let ext_id = store.data().package.manifest.id.clone();
        let mut loaded_extensions = self.loaded_extensions.lock();
        let entry = Entry::instantiate(&mut store, &wasm_component, &self.linker)
            .map_err(|e| ExtensionError::from_wasmtime(e, &ext_id))?;
        loaded_extensions.insert(ext_id, (store, entry));
        Ok(())
    }

    pub fn initialize_extension(&self, id: &str) -> Result<(), ExtensionError> {
        let mut loaded = self.loaded_extensions.lock();
        let (store, entry) = loaded
            .get_mut(id)
            .ok_or_else(|| anyhow::anyhow!("Extension not found: {id}"))?;
        entry
            .ark_core_entrance()
            .call_on_init(store)
            .map_err(|e| ExtensionError::from_wasmtime(e, id))?;
        Ok(())
    }

    pub fn initialize_extensions(&self) -> Result<(), ExtensionError> {
        // Collect IDs first so we do not hold the lock while calling into WASM.
        let ids: Vec<String> = {
            let loaded = self.loaded_extensions.lock();
            loaded.keys().cloned().collect()
        };

        let errors: Vec<ExtensionError> = ids
            .iter()
            .filter_map(|id| self.initialize_extension(id).err())
            .collect();

        if errors.is_empty() {
            Ok(())
        } else {
            Err(ExtensionError::Multi {
                count: errors.len(),
                errors,
            })
        }
    }

    pub fn disable_extension(&self, id: &str) -> Result<(), ExtensionError> {
        let mut loaded = self.loaded_extensions.lock();
        let (store, entry) = loaded
            .get_mut(id)
            .ok_or_else(|| anyhow::anyhow!("Extension not found: {id}"))?;
        self.disable_inner(store, entry, id)
    }

    fn disable_inner(
        &self,
        store: &mut Store<ExtensionContext>,
        entry: &mut Entry,
        id: &str,
    ) -> Result<(), ExtensionError> {
        // Remove this extension from all trigger registries.
        {
            let mut registry = self.registry.lock();
            registry.iter_mut().for_each(|(_trigger, entries)| {
                if let Some(pos) = entries.iter().position(|v| v == id) {
                    entries.swap_remove(pos);
                }
            });
        } // registry lock released before calling into WASM

        entry
            .ark_core_entrance()
            .call_on_destroy(store)
            .map_err(|e| ExtensionError::from_wasmtime(e, id))?;
        Ok(())
    }

    pub fn unload_extension(&self, id: &str) -> Result<(), ExtensionError> {
        let mut loaded = self.loaded_extensions.lock();
        if let Some((store, instance)) = loaded.get_mut(id) {
            self.disable_inner(store, instance, id)?;
        }
        loaded.remove(id);
        Ok(())
    }

    pub fn trigger_extension(&self, trigger: &str) -> Result<(), ExtensionError> {
        // Collect IDs under the registry lock, then release before calling into
        // WASM to avoid a lock-ordering deadlock (registry → loaded_extensions → registry).
        let ids: Vec<String> = {
            let registry = self.registry.lock();
            registry.get(trigger).cloned().unwrap_or_default()
        };

        for id in &ids {
            let mut loaded = self.loaded_extensions.lock();
            if let Some((store, instance)) = loaded.get_mut(id) {
                instance
                    .ark_core_entrance()
                    .call_on_callback(store, trigger)
                    .map_err(|e| ExtensionError::from_wasmtime(e, id))?;
            } else {
                warn!("Unable to find extension with id: {id} in trigger registry");
            }
        }
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct LaunchArgs {
    pub enabled_vulkan_extensions: Vec<String>,
    pub enabled_vulkan_features: Vec<String>,
    pub enabled_wasi_features: Vec<String>,
}

impl WasiView for ExtensionContext {
    fn ctx(&mut self) -> wasmtime_wasi::WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi_ctx,
            table: &mut self.table,
        }
    }
}

enum WasiFeature {
    Filesystem {
        host_path: String,
        guest_path: String,
    },
    Network {
        addrs: Vec<(IpAddr, Option<u16>)>,
    },
}

fn parse_wasi_feature(s: &str) -> anyhow::Result<WasiFeature> {
    if let Some(path) = s.strip_prefix("fs:") {
        let (host_path, guest_path) = path
            .split_once(':')
            .map(|(h, g)| (h.to_string(), g.to_string()))
            .unwrap_or_else(|| (path.to_string(), "/".to_string()));

        // Reject absolute paths
        if host_path.starts_with('/') {
            return Err(anyhow::anyhow!(
                "Absolute path not allowed for WASI fs feature: {}",
                host_path
            ));
        }

        // Canonicalize and verify the path stays within the current working directory
        let cwd = std::env::current_dir()?;
        let resolved = cwd.join(&host_path);
        let canonical = std::fs::canonicalize(&resolved).map_err(|e| {
            anyhow::anyhow!("Failed to resolve WASI fs path '{}': {}", host_path, e)
        })?;
        if !canonical.starts_with(&cwd) {
            return Err(anyhow::anyhow!(
                "WASI fs path '{}' escapes the program working directory",
                host_path
            ));
        }

        Ok(WasiFeature::Filesystem {
            host_path,
            guest_path,
        })
    } else if let Some(addr_str) = s.strip_prefix("net:") {
        if addr_str.is_empty() {
            return Err(anyhow::anyhow!(
                "Unconditional network access is not allowed"
            ));
        }
        let (host, port) = if let Some((h, p)) = addr_str.rsplit_once(':') {
            if let Ok(port) = p.parse::<u16>() {
                (h.to_string(), Some(port))
            } else {
                (addr_str.to_string(), None)
            }
        } else {
            (addr_str.to_string(), None)
        };
        let resolve_str = if let Some(port) = port {
            format!("{}:{}", host, port)
        } else {
            format!("{}:0", host)
        };
        let sock_addrs: Vec<SocketAddr> = resolve_str.to_socket_addrs()?.collect();
        if sock_addrs.is_empty() {
            return Err(anyhow::anyhow!(
                "Failed to resolve network address: {}",
                host
            ));
        }
        let addrs = sock_addrs.into_iter().map(|a| (a.ip(), port)).collect();
        Ok(WasiFeature::Network { addrs })
    } else {
        Err(anyhow::anyhow!("Unknown WASI feature: {}", s))
    }
}

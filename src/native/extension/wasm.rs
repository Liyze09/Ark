use std::{
    collections::HashMap,
    net::{IpAddr, SocketAddr, ToSocketAddrs},
    path::PathBuf,
    sync::{Arc, Mutex},
};

use log::error;
use rayon::iter::{IntoParallelRefMutIterator, ParallelIterator};
use wasmtime::{
    component::{Component, Linker},
    Engine, Store,
};
use wasmtime_wasi::{DirPerms, FilePerms, ResourceTable, WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

use crate::extension::package::{parse_package, ExtensionPackage};

pub struct WasmRuntime {
    pub engine: Engine,
    pub loaded_extensions: LoadedExtensions,
    pub linker: Linker<ExtensionContext>,
    pub extension_folder: String,
}

type LoadedExtensions =
    Arc<Mutex<HashMap<String, (Store<ExtensionContext>, wasmtime::component::Instance)>>>;

pub struct ExtensionContext {
    pub package: ExtensionPackage,
    pub wasm_component: Component,
    pub wasi_ctx: WasiCtx,
    pub table: ResourceTable,
}

impl WasmRuntime {
    pub fn new(extension_folder: String) -> Self {
        let engine = Engine::default();
        Self {
            linker: Linker::new(&engine),
            engine,
            loaded_extensions: Arc::new(Mutex::new(HashMap::new())),
            extension_folder,
        }
    }

    pub fn load_extension(&self, file_name: &str, args: LaunchArgs) -> anyhow::Result<()> {
        let path = PathBuf::new().join(&self.extension_folder).join(file_name);
        let bytes = std::fs::read(path)?;
        self.load_extension_by_bytes(&bytes, args)?;
        Ok(())
    }

    pub fn load_extension_by_bytes(&self, bytes: &[u8], args: LaunchArgs) -> anyhow::Result<()> {
        let package = parse_package(bytes)?;
        let wasm_bytes = package
            .files
            .get(package.manifest.entrypoint.as_str())
            .ok_or(anyhow::anyhow!(
                "Failed to find entrance wasm file in package"
            ))?;
        let wasm_component = Component::from_binary(&self.engine, wasm_bytes)?;

        let mut wasi_builder = WasiCtxBuilder::new();
        let mut network_addrs: Vec<(IpAddr, Option<u16>)> = Vec::new();

        for feature_str in &args.enabled_wasi_features {
            match parse_wasi_feature(feature_str)? {
                WasiFeature::Filesystem { host_path, guest_path } => {
                    wasi_builder.preopened_dir(
                        &host_path,
                        &guest_path,
                        DirPerms::all(),
                        FilePerms::all(),
                    )?;
                }
                WasiFeature::Network { addrs } => network_addrs.extend(addrs),
                WasiFeature::IpNameLookup => { wasi_builder.allow_ip_name_lookup(true); }
            }
        }

        if !network_addrs.is_empty() {
            let allowed = Arc::new(network_addrs);
            wasi_builder.socket_addr_check(move |addr, _use| {
                let allowed = Arc::clone(&allowed);
                Box::pin(async move {
                    allowed.iter().any(|(ip, port)| {
                        *ip == addr.ip() && port.is_none_or(|p| p == addr.port())
                    })
                })
            });
        }

        let mut store = Store::new(
            &self.engine,
            ExtensionContext {
                package,
                wasm_component: wasm_component.clone(),
                wasi_ctx: wasi_builder.build(),
                table: ResourceTable::new(),
            },
        );
        let instance = self.linker.instantiate(&mut store, &wasm_component)?;
        let mut loaded_extensions = self.loaded_extensions.lock().unwrap();
        loaded_extensions.insert(store.data().package.manifest.id.clone(), (store, instance));
        Ok(())
    }

    pub fn initialize_extension(&self, id: &str) -> anyhow::Result<()> {
        let mut binding = self.loaded_extensions.lock().unwrap();
        let (store, instance) = binding
            .get_mut(id)
            .ok_or(anyhow::anyhow!("Failed to find extension with id: {}", id))?;
        let fun_name = store.data().package.manifest.entry_function.clone();
        if let Some(fun) = instance.get_func(&mut *store, &fun_name) {
            fun.call(store, &[], &mut [])?;
        }
        Ok(())
    }

    pub fn initialize_extensions(&self) -> anyhow::Result<()> {
        self.loaded_extensions
            .lock()
            .unwrap()
            .par_iter_mut()
            .for_each(|(_name, (store, instance))| {
                let fun_name = store.data().package.manifest.entry_function.clone();
                if let Some(fun) = instance.get_func(&mut *store, &fun_name) {
                    let result = fun.call(store, &[], &mut []);
                    if let Err(result) = result {
                        error!("Failed to initialize extension: {:?}", result)
                    }
                };
            });
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

impl Default for WasmRuntime {
    fn default() -> Self {
        Self::new(String::new())
    }
}

enum WasiFeature {
    Filesystem { host_path: String, guest_path: String },
    Network { addrs: Vec<(IpAddr, Option<u16>)> },
    IpNameLookup,
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

        Ok(WasiFeature::Filesystem { host_path, guest_path })
    } else if let Some(addr_str) = s.strip_prefix("net:") {
        if addr_str.is_empty() {
            return Err(anyhow::anyhow!("Unconditional network access is not allowed"));
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
            return Err(anyhow::anyhow!("Failed to resolve network address: {}", host));
        }
        let addrs = sock_addrs.into_iter().map(|a| (a.ip(), port)).collect();
        Ok(WasiFeature::Network { addrs })
    } else if s == "ip_name_lookup" {
        Ok(WasiFeature::IpNameLookup)
    } else {
        Err(anyhow::anyhow!("Unknown WASI feature: {}", s))
    }
}

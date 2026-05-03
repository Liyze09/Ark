use std::{collections::HashMap, path::{PathBuf}, sync::{Arc, Mutex}};

use log::error;
use rayon::{iter::{IntoParallelRefMutIterator, ParallelIterator}};
use wasmtime::{Engine, Store, component::{Component, Linker}};

use crate::extension::package::{ExtensionPackage, parse_package};

pub struct WasmRuntime {
    pub engine: Engine,
    pub loaded_extensions: LoadedExtensions,
    pub linker: Linker<ExtensionContext>,
    pub extension_folder: String,
}

type LoadedExtensions =  Arc<Mutex<HashMap<String,(Store<ExtensionContext>, wasmtime::component::Instance)>>>;

pub struct ExtensionContext {
    pub package: ExtensionPackage,
    pub wasm_component: Component,
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

    pub fn load_extension(&self, file_name: &str) -> anyhow::Result<()> {
        let path = PathBuf::new().join(&self.extension_folder).join(file_name);
        let bytes = std::fs::read(path)?;
        self.load_extension_by_bytes(&bytes)?;
        Ok(())
    }

    pub fn load_extension_by_bytes(&self, bytes: &[u8]) -> anyhow::Result<()> {
        let package = parse_package(bytes)?;
        let wasm_bytes = package.files.get(package.manifest.entrypoint.as_str()).ok_or(anyhow::anyhow!("Failed to find entrance wasm file in package"))?;
        let wasm_component = Component::from_binary(&self.engine,  wasm_bytes)?;
        let mut store = Store::new(&self.engine, ExtensionContext { 
            package, 
            wasm_component: wasm_component.clone() });
        let instance = self.linker.instantiate(&mut store, &wasm_component)?;
        let mut loaded_extensions = self.loaded_extensions.lock().unwrap();
        loaded_extensions.insert(store.data().package.manifest.id.clone(), (store, instance));
        Ok(())
    }

    pub fn initialize_extension(&self, id: &str) -> anyhow::Result<()> {
        let mut binding = self.loaded_extensions.lock().unwrap();
        let (store, instance) = binding.get_mut(id).ok_or(anyhow::anyhow!("Failed to find extension with id: {}", id))?;
        let fun_name = store.data().package.manifest.entry_function.clone();
        if let Some(fun) = instance.get_func(&mut *store, &fun_name) {
            fun.call(store, &[], &mut [])?;
        }
        Ok(())
    }

    pub fn initialize_extensions(&self) -> anyhow::Result<()> {
        self.loaded_extensions.lock().unwrap().par_iter_mut().for_each(|(_name, (store, instance))| {
            let fun_name = store.data().package.manifest.entry_function.clone();
            if let Some(fun) = instance.get_func(&mut *store, &fun_name) {
                let result = fun.call(store, &[], &mut []);
                if let Err(result) =  result {
                    error!("Failed to initialize extension: {:?}", result)
                }
            };
        });
        Ok(())
    }
}

impl Default for WasmRuntime {
    fn default() -> Self {
        Self::new(String::new())
    }
}
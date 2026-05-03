pub mod vulkan;
pub mod shaders;
pub mod extension;

use std::ffi::{CStr, CString};
use std::sync::Mutex;

use anyhow::anyhow;
use mimalloc::MiMalloc;
use vulkanalia::{Entry, loader::{LIBRARY, LibloadingLoader}, vk};
use vulkanalia_vma::vma::VmaAllocator;

use crate::{extension::wasm::WasmRuntime, vulkan::VkBackend};

#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

pub struct NativeContext {
    pub vulkan_backend: VkBackend,
    pub wasm_runtime: WasmRuntime,
    errors: Mutex<Vec<anyhow::Error>>,
}

impl NativeContext {
    /// # Safety
    ///  - `instance_handle`, `device_handle`, `vma_handle`, `transfer_queue`, `graphics_queue`, `compute_queue` must be valid pointers to Vulkan objects.
    pub unsafe fn new(
        instance: vk::Instance,
        device: vk::Device,
        vma: VmaAllocator,
        transfer_queue: vk::Queue,
        graphics_queue: vk::Queue,
        compute_queue: vk::Queue,
        extension_folder: String,
    ) -> anyhow::Result<Self> {
        let loader = unsafe { LibloadingLoader::new(LIBRARY)? };
        let entry = unsafe { Entry::new(loader).map_err(|b| anyhow!("{}", b))? };
        let vulkan_backend = VkBackend {
            entry,
            instance,
            device,
            vma,
            transfer_queue,
            graphics_queue,
            compute_queue
        };
        Ok( Self {
            vulkan_backend,
            wasm_runtime: WasmRuntime::new(extension_folder),
            errors: Mutex::new(Vec::new()),
        })
    }

    pub fn push_error(&self, err: anyhow::Error) {
        self.errors.lock().unwrap().push(err);
    }

    pub fn pop_error(&self) -> Option<anyhow::Error> {
        self.errors.lock().unwrap().pop()
    }

    pub fn error_count(&self) -> usize {
        self.errors.lock().unwrap().len()
    }
}

/// # Safety
/// All parameters must be valid Vulkan object handles.
/// Returns a pointer to a heap-allocated `NativeContext` as an `i64`, or `0` on failure.
/// Designed for Java FFM API interop — callers must eventually free the returned pointer
/// via `ark_destroy_native_context`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ark_create_native_context(
    instance_handle: i64,
    device_handle: i64,
    vma_handle: i64,
    transfer_queue: i64,
    graphics_queue: i64,
    compute_queue: i64,
    extension_folder: *const std::ffi::c_char,
) -> i64 {
    let folder = if extension_folder.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(extension_folder) }.to_string_lossy().into_owned()
    };
    let result = unsafe {
        NativeContext::new(
            std::mem::transmute::<usize, vk::Instance>(instance_handle as usize),
            std::mem::transmute::<usize, vk::Device>(device_handle as usize),
            std::mem::transmute::<usize, VmaAllocator>(vma_handle as usize),
            std::mem::transmute::<usize, vk::Queue>(transfer_queue as usize),
            std::mem::transmute::<usize, vk::Queue>(graphics_queue as usize),
            std::mem::transmute::<usize, vk::Queue>(compute_queue as usize),
            folder,
        )
    };
    match result {
        Ok(ctx) => {
            let ptr = Box::into_raw(Box::new(ctx));
            ptr as i64
        }
        Err(e) => {
            eprintln!("[Ark] Failed to create NativeContext: {e}");
            0
        }
    }
}

/// # Safety
/// `ptr` must be a pointer previously returned by `ark_create_native_context`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ark_destroy_native_context(ptr: i64) {
    if ptr != 0 {
        drop(unsafe { Box::from_raw(ptr as *mut NativeContext) });
    }
}

/// # Safety
/// `ptr` must be a pointer previously returned by `ark_create_native_context`.
/// `file_name` must be a valid C string.
pub unsafe extern "C" fn ark_load_extension_file(ptr: i64, file_name: *const std::ffi::c_char) -> i32 {
    let ctx = unsafe { &mut *(ptr as *mut NativeContext) };
    match ctx.wasm_runtime.load_extension(unsafe { CStr::from_ptr(file_name) }.to_string_lossy().as_ref()) {
        Ok(_) => 0,
        Err(e) => {
            ctx.push_error(e);
            1
        }
    }
}

/// # Safety
/// `ptr` must be a valid pointer returned by `ark_create_native_context`.
/// Returns a heap-allocated C string with the most recent error's chain-formatted message,
/// or null if no errors are stored. The caller must free the string via `ark_free_string`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ark_pop_error(ptr: i64) -> *mut std::ffi::c_char {
    let ctx = unsafe { &mut *(ptr as *mut NativeContext) };
    match ctx.pop_error() {
        Some(err) => {
            let msg = format!("{err:#}");
            CString::new(msg).unwrap_or_else(|_| CString::new("error contains nul byte").unwrap()).into_raw()
        }
        None => std::ptr::null_mut(),
    }
}

/// # Safety
/// `ptr` must be a valid pointer returned by `ark_create_native_context`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ark_error_count(ptr: i64) -> i32 {
    let ctx = unsafe { &mut *(ptr as *mut NativeContext) };
    ctx.error_count() as i32
}

/// # Safety
/// `ptr` must be a string previously returned by `ark_pop_error`, or null (no-op).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ark_free_string(ptr: *mut std::ffi::c_char) {
    if !ptr.is_null() {
        drop(unsafe { CString::from_raw(ptr) });
    }
}


use ark_runtime::{LaunchArgs, VkBackend, WasmRuntime};

use std::ffi::{CStr, CString};

use mimalloc::MiMalloc;
use parking_lot::Mutex;
use vulkanalia::{
    loader::{LIBRARY, LibloadingLoader, Loader}, vk
};
use vulkanalia_vma::vma::VmaAllocator;


#[global_allocator]
static GLOBAL: MiMalloc = MiMalloc;

// ── Log/panic callback types (passed from Java via FFM upcall stubs) ──────────

type LogFn = unsafe extern "C" fn(*const std::ffi::c_char);
type FatalFn = unsafe extern "C" fn(*const std::ffi::c_char);

struct LoggerFuncs {
    trace: LogFn,
    debug: LogFn,
    info: LogFn,
    warn: LogFn,
    error: LogFn,
}

static LOGGER_FUNCS: std::sync::OnceLock<LoggerFuncs> = std::sync::OnceLock::new();
static FATAL_HANDLER: std::sync::OnceLock<FatalFn> = std::sync::OnceLock::new();

struct ArkLogger;

impl log::Log for ArkLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &log::Record) {
        let Some(f) = LOGGER_FUNCS.get() else { return };
        let msg = match CString::new(format!("{}", record.args())) {
            Ok(c) => c,
            Err(_) => CString::new("(log message contained null byte)").unwrap(),
        };
        let func = match record.level() {
            log::Level::Trace => f.trace,
            log::Level::Debug => f.debug,
            log::Level::Info => f.info,
            log::Level::Warn => f.warn,
            log::Level::Error => f.error,
        };
        unsafe { (func)(msg.as_ptr()) };
    }

    fn flush(&self) {}
}

fn init_logger(trace: LogFn, debug: LogFn, info: LogFn, warn: LogFn, error: LogFn) -> bool {
    let funcs = LoggerFuncs { trace, debug, info, warn, error };
    if LOGGER_FUNCS.set(funcs).is_err() {
        return false; // already initialized
    }
    let _ = log::set_logger(&ArkLogger);
    log::set_max_level(log::LevelFilter::Trace);
    true
}

fn invoke_fatal(msg: &str) {
    if let Some(fatal) = FATAL_HANDLER.get() {
        let cmsg = CString::new(msg).unwrap_or_else(|_| CString::new("(unknown)").unwrap());
        unsafe { (fatal)(cmsg.as_ptr()) };
    } else {
        eprintln!("[Ark] FATAL (no handler): {msg}");
    }
}

fn handle_ffi_panic(panic: Box<dyn std::any::Any + Send>) {
    let msg = panic
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| panic.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| format!("{:?}", panic));
    invoke_fatal(&msg);
}

macro_rules! ffi_catch {
    ($body:expr, $on_panic:expr) => {{
        let __result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| $body));
        match __result {
            Ok(__v) => __v,
            Err(__panic) => {
                handle_ffi_panic(__panic);
                $on_panic
            }
        }
    }};
}

pub struct NativeContext {
    pub wasm_runtime: WasmRuntime,
    pub errors: Mutex<Vec<anyhow::Error>>,
}

impl NativeContext {
    /// # Safety
    ///  - `instance_handle`, `device_handle`, `vma_handle`, `graphics_queue`, `compute_queue`, `transfer_queue` must be valid pointers to Vulkan objects.
    #[allow(clippy::too_many_arguments)]
    pub unsafe fn new(
        instance: vk::Instance,
        device: vk::Device,
        vma: VmaAllocator,
        graphics_queue: vk::Queue,
        compute_queue: vk::Queue,
        transfer_queue: vk::Queue,
        graphics_queue_family_index: u32,
        compute_queue_family_index: u32,
        transfer_queue_family_index: u32,
        extension_folder: String,
    ) -> anyhow::Result<Self> {
        let loader = unsafe { LibloadingLoader::new(LIBRARY)? };
        let get_instance_proc_addr: vk::PFN_vkGetInstanceProcAddr = unsafe {
            std::mem::transmute(
                loader.load(b"vkGetInstanceProcAddr\0")
                    .expect("vkGetInstanceProcAddr not found")
            )
        };
        let get_device_proc_addr: vk::PFN_vkGetDeviceProcAddr = unsafe {
            std::mem::transmute(get_instance_proc_addr(instance, c"vkGetDeviceProcAddr".as_ptr()))
        };
        let device_commands = unsafe {
            vk::DeviceCommands::load(|name| get_device_proc_addr(device, name))
        };

        let vulkan_backend = VkBackend {
            instance,
            device,
            device_commands,
            vma,
            transfer_queue,
            graphics_queue,
            compute_queue,
            graphics_queue_family_index,
            compute_queue_family_index,
            transfer_queue_family_index,
        };
        Ok(Self {
            wasm_runtime: WasmRuntime::new(extension_folder, vulkan_backend.clone())?,
            errors: Mutex::new(Vec::new()),
        })
    }

    pub fn push_error(&self, err: impl Into<anyhow::Error>) {
        self.errors.lock().push(err.into());
    }

    pub fn pop_error(&self) -> Option<anyhow::Error> {
        self.errors.lock().pop()
    }

    pub fn error_count(&self) -> usize {
        self.errors.lock().len()
    }
}

/// # Safety
/// All function pointer parameters must be valid FFM upcall stubs.
/// Must be called exactly once, immediately after loading the native library,
/// before any other Ark functions. Both `OnceLock`s reject duplicate calls.
/// Returns 0 on success, 1 if already initialized.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ark_init_callbacks(
    log_trace: LogFn,
    log_debug: LogFn,
    log_info: LogFn,
    log_warn: LogFn,
    log_error: LogFn,
    fatal_handler: FatalFn,
) -> i32 {
    // Register fatal handler FIRST so it's available if logger init panics
    let _ = FATAL_HANDLER.set(fatal_handler);
    if init_logger(log_trace, log_debug, log_info, log_warn, log_error) {
        0
    } else {
        1 // already initialized
    }
}

/// # Safety
/// All parameters must be valid Vulkan object handles.
/// `ark_init_callbacks` must have been called first.
/// Returns a pointer to a heap-allocated `NativeContext` as an `i64`, or `0` on failure.
/// Designed for Java FFM API interop — callers must eventually free the returned pointer
/// via `ark_destroy_native_context`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ark_create_native_context(
    instance_handle: i64,
    device_handle: i64,
    vma_handle: i64,
    graphics_queue: i64,
    compute_queue: i64,
    transfer_queue: i64,
    graphics_queue_family_index: i32,
    compute_queue_family_index: i32,
    transfer_queue_family_index: i32,
    extension_folder: *const std::ffi::c_char,
) -> i64 {
    let folder = if extension_folder.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(extension_folder) }
            .to_string_lossy()
            .into_owned()
    };

    ffi_catch!(
        {
            let result = unsafe {
                NativeContext::new(
                    std::mem::transmute::<usize, vk::Instance>(instance_handle as usize),
                    std::mem::transmute::<usize, vk::Device>(device_handle as usize),
                    std::mem::transmute::<usize, VmaAllocator>(vma_handle as usize),
                    std::mem::transmute::<usize, vk::Queue>(graphics_queue as usize),
                    std::mem::transmute::<usize, vk::Queue>(compute_queue as usize),
                    std::mem::transmute::<usize, vk::Queue>(transfer_queue as usize),
                    graphics_queue_family_index as u32,
                    compute_queue_family_index as u32,
                    transfer_queue_family_index as u32,
                    folder,
                )
            };
            match result {
                Ok(ctx) => {
                    let ptr = Box::into_raw(Box::new(ctx));
                    ptr as i64
                }
                Err(e) => {
                    log::error!("Failed to create NativeContext: {e}");
                    0i64
                }
            }
        },
        0i64
    )
}

/// # Safety
/// `ptr` must be a pointer previously returned by `ark_create_native_context`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ark_destroy_native_context(ptr: i64) {
    ffi_catch!(
        {
            if ptr != 0 {
                drop(unsafe { Box::from_raw(ptr as *mut NativeContext) });
            }
        },
        ()
    )
}

/// # Safety
/// `ptr` must be a pointer previously returned by `ark_create_native_context`.
/// `file_name` and `wasi_features_json` must be valid C strings (or null).
/// `wasi_features_json` is a JSON array of WASI feature strings, e.g. `["fs:./data"]`.
/// Returns 0 on success, 1 on failure (use `ark_pop_error` to retrieve the error).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ark_load_extension(
    ptr: i64,
    file_name: *const std::ffi::c_char,
    wasi_features_json: *const std::ffi::c_char,
) -> i32 {
    ffi_catch!(
        {
            let ctx = unsafe { &mut *(ptr as *mut NativeContext) };
            let file_name = unsafe { CStr::from_ptr(file_name) }.to_string_lossy();
            let wasi_features: Vec<String> = if wasi_features_json.is_null() {
                Vec::new()
            } else {
                let json = unsafe { CStr::from_ptr(wasi_features_json) }.to_string_lossy();
                match serde_json::from_str(&json) {
                    Ok(v) => v,
                    Err(e) => {
                        ctx.push_error(anyhow::anyhow!("Failed to parse wasi_features JSON: {}", e));
                        return 1;
                    }
                }
            };
            match ctx.wasm_runtime.load_extension(
                file_name.as_ref(),
                LaunchArgs { enabled_wasi_features: wasi_features, ..Default::default() },
            ) {
                Ok(_) => 0,
                Err(e) => {
                    ctx.push_error(e);
                    1
                }
            }
        },
        1
    )
}

/// # Safety
/// `ptr` must be a pointer previously returned by `ark_create_native_context`.
/// `id` must be a valid C string. Returns 0 on success, 1 on failure.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ark_initialize_extension(ptr: i64, id: *const std::ffi::c_char) -> i32 {
    ffi_catch!(
        {
            let ctx = unsafe { &mut *(ptr as *mut NativeContext) };
            let id = unsafe { CStr::from_ptr(id) }.to_string_lossy();
            match ctx.wasm_runtime.initialize_extension(&id) {
                Ok(_) => 0,
                Err(e) => {
                    ctx.push_error(e);
                    1
                }
            }
        },
        1
    )
}

/// # Safety
/// `ptr` must be a pointer previously returned by `ark_create_native_context`.
/// Returns 0 on success, 1 on failure.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ark_initialize_extensions(ptr: i64) -> i32 {
    ffi_catch!(
        {
            let ctx = unsafe { &mut *(ptr as *mut NativeContext) };
            match ctx.wasm_runtime.initialize_extensions() {
                Ok(_) => 0,
                Err(e) => {
                    ctx.push_error(e);
                    1
                }
            }
        },
        1
    )
}

/// # Safety
/// `ptr` must be a pointer previously returned by `ark_create_native_context`.
/// `id` must be a valid C string. Returns 0 on success, 1 on failure.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ark_disable_extension(ptr: i64, id: *const std::ffi::c_char) -> i32 {
    ffi_catch!(
        {
            let ctx = unsafe { &mut *(ptr as *mut NativeContext) };
            let id = unsafe { CStr::from_ptr(id) }.to_string_lossy();
            match ctx.wasm_runtime.disable_extension(&id) {
                Ok(_) => 0,
                Err(e) => {
                    ctx.push_error(e);
                    1
                }
            }
        },
        1
    )
}

/// # Safety
/// `ptr` must be a pointer previously returned by `ark_create_native_context`.
/// `id` must be a valid C string. Returns 0 on success, 1 on failure.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ark_unload_extension(ptr: i64, id: *const std::ffi::c_char) -> i32 {
    ffi_catch!(
        {
            let ctx = unsafe { &mut *(ptr as *mut NativeContext) };
            let id = unsafe { CStr::from_ptr(id) }.to_string_lossy();
            match ctx.wasm_runtime.unload_extension(&id) {
                Ok(_) => 0,
                Err(e) => {
                    ctx.push_error(e);
                    1
                }
            }
        },
        1
    )
}

/// # Safety
/// `ptr` must be a valid pointer returned by `ark_create_native_context`.
/// Returns a heap-allocated C string with the most recent error's chain-formatted message,
/// or null if no errors are stored. The caller must free the string via `ark_free_string`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ark_pop_error(ptr: i64) -> *mut std::ffi::c_char {
    ffi_catch!(
        {
            let ctx = unsafe { &mut *(ptr as *mut NativeContext) };
            match ctx.pop_error() {
                Some(err) => {
                    let msg = format!("{err:#}");
                    CString::new(msg)
                        .unwrap_or_else(|_| CString::new("error contains nul byte").unwrap())
                        .into_raw()
                }
                None => std::ptr::null_mut(),
            }
        },
        std::ptr::null_mut()
    )
}

/// # Safety
/// `ptr` must be a valid pointer returned by `ark_create_native_context`.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ark_error_count(ptr: i64) -> i32 {
    ffi_catch!(
        {
            let ctx = unsafe { &mut *(ptr as *mut NativeContext) };
            ctx.error_count() as i32
        },
        0
    )
}

/// # Safety
/// `ptr` must be a string previously returned by `ark_pop_error`, or null (no-op).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ark_free_string(ptr: *mut std::ffi::c_char) {
    ffi_catch!(
        {
            if !ptr.is_null() {
                drop(unsafe { CString::from_raw(ptr) });
            }
        },
        ()
    )
}

/// # Safety
/// `ptr` must be a valid pointer returned by `ark_create_native_context`.
/// `json` must be a valid C string containing a JSON array of feature names,
/// e.g. `["timelineSemaphore","hostQueryReset"]`, or null to clear the set.
/// Returns 0 on success, 1 on failure (use `ark_pop_error`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ark_set_enabled_vulkan_features(
    ptr: i64,
    json: *const std::ffi::c_char,
) -> i32 {
    ffi_catch!(
        {
            let ctx = unsafe { &mut *(ptr as *mut NativeContext) };
            let features: Vec<String> = if json.is_null() {
                Vec::new()
            } else {
                let json_str = unsafe { CStr::from_ptr(json) }.to_string_lossy();
                match serde_json::from_str(&json_str) {
                    Ok(v) => v,
                    Err(e) => {
                        ctx.push_error(anyhow::anyhow!("Failed to parse enabled vulkan features JSON: {e}"));
                        return 1;
                    }
                }
            };
            let mut set = ctx.wasm_runtime.enabled_vulkan_features.lock();
            set.clear();
            set.extend(features);
            0
        },
        1
    )
}

/// # Safety
/// `ptr` must be a valid pointer returned by `ark_create_native_context`.
/// `json` must be a valid C string containing a JSON array of extension names,
/// e.g. `["VK_KHR_swapchain","VK_EXT_descriptor_indexing"]`, or null to clear the set.
/// Returns 0 on success, 1 on failure (use `ark_pop_error`).
#[unsafe(no_mangle)]
pub unsafe extern "C" fn ark_set_enabled_vulkan_extensions(
    ptr: i64,
    json: *const std::ffi::c_char,
) -> i32 {
    ffi_catch!(
        {
            let ctx = unsafe { &mut *(ptr as *mut NativeContext) };
            let extensions: Vec<String> = if json.is_null() {
                Vec::new()
            } else {
                let json_str = unsafe { CStr::from_ptr(json) }.to_string_lossy();
                match serde_json::from_str(&json_str) {
                    Ok(v) => v,
                    Err(e) => {
                        ctx.push_error(anyhow::anyhow!("Failed to parse enabled vulkan extensions JSON: {e}"));
                        return 1;
                    }
                }
            };
            let mut set = ctx.wasm_runtime.enabled_vulkan_extensions.lock();
            set.clear();
            set.extend(extensions);
            0
        },
        1
    )
}

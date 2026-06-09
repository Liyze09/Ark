package io.github.liyze09.ark;

import com.mojang.blaze3d.vulkan.VulkanQueue;
import io.github.liyze09.ark.exception.FatalNativeException;
import io.github.liyze09.ark.exception.NativeException;
import org.jetbrains.annotations.NotNull;
import org.jspecify.annotations.NonNull;
import org.jspecify.annotations.Nullable;

import java.lang.foreign.*;
import java.lang.invoke.MethodHandle;
import java.lang.invoke.MethodHandles;
import java.lang.invoke.MethodType;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.concurrent.atomic.AtomicBoolean;

public final class NativeContext {
    private static final MethodHandle CREATE_NATIVE_CONTEXT;
    private static final MethodHandle DESTROY_NATIVE_CONTEXT;
    private static final MethodHandle LOAD_EXTENSION;
    private static final MethodHandle INITIALIZE_EXTENSION;
    private static final MethodHandle INITIALIZE_EXTENSIONS;
    private static final MethodHandle DISABLE_EXTENSION;
    private static final MethodHandle UNLOAD_EXTENSION;
    private static final MethodHandle POP_ERROR;
    private static final MethodHandle ERROR_COUNT;
    private static final MethodHandle SET_ENABLED_VULKAN_FEATURES;
    private static final MethodHandle SET_ENABLED_VULKAN_EXTENSIONS;
    private static final MethodHandle FREE_STRING;

    // ── Log/fatal upcall stub handles ───────────────────────────────────────

    private static final MethodHandle LOG_TRACE_HANDLE;
    private static final MethodHandle LOG_DEBUG_HANDLE;
    private static final MethodHandle LOG_INFO_HANDLE;
    private static final MethodHandle LOG_WARN_HANDLE;
    private static final MethodHandle LOG_ERROR_HANDLE;
    private static final MethodHandle FATAL_HANDLE;

    private static final FunctionDescriptor LOG_FUNC_DESC =
            FunctionDescriptor.ofVoid(ValueLayout.ADDRESS);

    static {
        try {
            System.loadLibrary("ark");
            var linker = Linker.nativeLinker();
            var lookup = SymbolLookup.loaderLookup();
            var mhLookup = MethodHandles.lookup();

            // ── 1. Create log/fatal upcall stubs and call ark_init_callbacks ──

            LOG_TRACE_HANDLE = mhLookup.findStatic(NativeContext.class, "onLogTrace",
                    MethodType.methodType(void.class, MemorySegment.class));
            LOG_DEBUG_HANDLE = mhLookup.findStatic(NativeContext.class, "onLogDebug",
                    MethodType.methodType(void.class, MemorySegment.class));
            LOG_INFO_HANDLE = mhLookup.findStatic(NativeContext.class, "onLogInfo",
                    MethodType.methodType(void.class, MemorySegment.class));
            LOG_WARN_HANDLE = mhLookup.findStatic(NativeContext.class, "onLogWarn",
                    MethodType.methodType(void.class, MemorySegment.class));
            LOG_ERROR_HANDLE = mhLookup.findStatic(NativeContext.class, "onLogError",
                    MethodType.methodType(void.class, MemorySegment.class));
            FATAL_HANDLE = mhLookup.findStatic(NativeContext.class, "onFatal",
                    MethodType.methodType(void.class, MemorySegment.class));

            // Arena that lives for the JVM lifetime — stubs must never be freed
            var callbacksArena = Arena.ofShared();

            var logTraceStub = linker.upcallStub(LOG_TRACE_HANDLE, LOG_FUNC_DESC, callbacksArena);
            var logDebugStub = linker.upcallStub(LOG_DEBUG_HANDLE, LOG_FUNC_DESC, callbacksArena);
            var logInfoStub = linker.upcallStub(LOG_INFO_HANDLE, LOG_FUNC_DESC, callbacksArena);
            var logWarnStub = linker.upcallStub(LOG_WARN_HANDLE, LOG_FUNC_DESC, callbacksArena);
            var logErrorStub = linker.upcallStub(LOG_ERROR_HANDLE, LOG_FUNC_DESC, callbacksArena);
            var fatalStub = linker.upcallStub(FATAL_HANDLE, LOG_FUNC_DESC, callbacksArena);

            var initCallbacksSymbol = lookup.find("ark_init_callbacks").orElseThrow();
            var initCallbacksHandle = linker.downcallHandle(
                    initCallbacksSymbol,
                    FunctionDescriptor.of(
                            ValueLayout.JAVA_INT,
                            ValueLayout.ADDRESS, // log_trace
                            ValueLayout.ADDRESS, // log_debug
                            ValueLayout.ADDRESS, // log_info
                            ValueLayout.ADDRESS, // log_warn
                            ValueLayout.ADDRESS, // log_error
                            ValueLayout.ADDRESS  // fatal_handler
                    )
            );
            int rc = (int) initCallbacksHandle.invokeExact(
                    logTraceStub, logDebugStub, logInfoStub,
                    logWarnStub, logErrorStub, fatalStub
            );
            if (rc != 0) {
                Ark.LOGGER.warn("ark_init_callbacks was already initialized");
            }

            // ── 2. Downcall handles for all other native functions ────────────

            var createSymbol = lookup.find("ark_create_native_context").orElseThrow();
            CREATE_NATIVE_CONTEXT = linker.downcallHandle(
                    createSymbol,
                    FunctionDescriptor.of(
                            ValueLayout.JAVA_LONG,
                            ValueLayout.JAVA_LONG, ValueLayout.JAVA_LONG,   // instance, device
                            ValueLayout.JAVA_LONG, ValueLayout.JAVA_LONG,   // vma, graphicsQueue
                            ValueLayout.JAVA_LONG, ValueLayout.JAVA_LONG,   // computeQueue, transferQueue
                            ValueLayout.JAVA_INT, ValueLayout.JAVA_INT,     // graphicsQFI, computeQFI
                            ValueLayout.JAVA_INT, ValueLayout.ADDRESS       // transferQFI, path
                    )
            );

            var destroySymbol = lookup.find("ark_destroy_native_context").orElseThrow();
            DESTROY_NATIVE_CONTEXT = linker.downcallHandle(
                    destroySymbol,
                    FunctionDescriptor.ofVoid(ValueLayout.JAVA_LONG)
            );

            var popErrorSymbol = lookup.find("ark_pop_error").orElseThrow();
            POP_ERROR = linker.downcallHandle(
                    popErrorSymbol,
                    FunctionDescriptor.of(ValueLayout.ADDRESS, ValueLayout.JAVA_LONG)
            );

            var errorCountSymbol = lookup.find("ark_error_count").orElseThrow();
            ERROR_COUNT = linker.downcallHandle(
                    errorCountSymbol,
                    FunctionDescriptor.of(ValueLayout.JAVA_INT, ValueLayout.JAVA_LONG)
            );

            var loadExtSymbol = lookup.find("ark_load_extension").orElseThrow();
            LOAD_EXTENSION = linker.downcallHandle(
                    loadExtSymbol,
                    FunctionDescriptor.of(ValueLayout.JAVA_INT, ValueLayout.JAVA_LONG,
                            ValueLayout.ADDRESS, ValueLayout.ADDRESS)
            );

            var initExtSymbol = lookup.find("ark_initialize_extension").orElseThrow();
            INITIALIZE_EXTENSION = linker.downcallHandle(
                    initExtSymbol,
                    FunctionDescriptor.of(ValueLayout.JAVA_INT, ValueLayout.JAVA_LONG,
                            ValueLayout.ADDRESS)
            );

            var initExtsSymbol = lookup.find("ark_initialize_extensions").orElseThrow();
            INITIALIZE_EXTENSIONS = linker.downcallHandle(
                    initExtsSymbol,
                    FunctionDescriptor.of(ValueLayout.JAVA_INT, ValueLayout.JAVA_LONG)
            );

            var disableExtSymbol = lookup.find("ark_disable_extension").orElseThrow();
            DISABLE_EXTENSION = linker.downcallHandle(
                    disableExtSymbol,
                    FunctionDescriptor.of(ValueLayout.JAVA_INT, ValueLayout.JAVA_LONG,
                            ValueLayout.ADDRESS)
            );

            var unloadExtSymbol = lookup.find("ark_unload_extension").orElseThrow();
            UNLOAD_EXTENSION = linker.downcallHandle(
                    unloadExtSymbol,
                    FunctionDescriptor.of(ValueLayout.JAVA_INT, ValueLayout.JAVA_LONG,
                            ValueLayout.ADDRESS)
            );

            var setFeaturesSymbol = lookup.find("ark_set_enabled_vulkan_features").orElseThrow();
            SET_ENABLED_VULKAN_FEATURES = linker.downcallHandle(
                    setFeaturesSymbol,
                    FunctionDescriptor.of(ValueLayout.JAVA_INT, ValueLayout.JAVA_LONG,
                            ValueLayout.ADDRESS)
            );

            var setExtsSymbol = lookup.find("ark_set_enabled_vulkan_extensions").orElseThrow();
            SET_ENABLED_VULKAN_EXTENSIONS = linker.downcallHandle(
                    setExtsSymbol,
                    FunctionDescriptor.of(ValueLayout.JAVA_INT, ValueLayout.JAVA_LONG,
                            ValueLayout.ADDRESS)
            );

            var freeStringSymbol = lookup.find("ark_free_string").orElseThrow();
            FREE_STRING = linker.downcallHandle(
                    freeStringSymbol,
                    FunctionDescriptor.ofVoid(ValueLayout.ADDRESS)
            );
        } catch (Throwable e) {
            throw new ExceptionInInitializerError(e);
        }
    }

    // ── Log/fatal callbacks (invoked from Rust side via FFM upcall stubs) ──

    private static void onLogTrace(@NonNull MemorySegment msg) {
        Ark.LOGGER.trace(msg.getString(0));
    }

    private static void onLogDebug(@NonNull MemorySegment msg) {
        Ark.LOGGER.debug(msg.getString(0));
    }

    private static void onLogInfo(@NonNull MemorySegment msg) {
        Ark.LOGGER.info(msg.getString(0));
    }

    private static void onLogWarn(@NonNull MemorySegment msg) {
        Ark.LOGGER.warn(msg.getString(0));
    }

    private static void onLogError(@NonNull MemorySegment msg) {
        Ark.LOGGER.error(msg.getString(0));
    }

    private static void onFatal(@NonNull MemorySegment msg) {
        Ark.LOGGER.error("[Ark] [FATAL] A Rust panic occurred: {}", msg.getString(0));
        var ctx = Ark.getNativeContext();
        if (ctx != null) {
            ctx.markDefunct();
        }
    }

    // ── Instance state ─────────────────────────────────────────────────────

    private final long address;
    private final AtomicBoolean destroyed = new AtomicBoolean(false);
    private volatile boolean defunct;
    private volatile List<String> fatalErrors = List.of();

    private NativeContext(long address) {
        this.address = address;
    }

    // ── Fatal error handling ───────────────────────────────────────────────

    /**
     * Called by the static {@link #onFatal} callback (from native panic handler).
     * Collects all pending errors from the Rust side exactly once, then marks
     * this context as defunct.
     * Must not call {@link #destroy()} because the native call that triggered
     * the panic is still on the stack — the context is freed later in {@link #exit()}.
     */
    void markDefunct() {
        if (this.defunct) {
            return; // already marked — onFatal may fire multiple times
        }
        // Drain errors while the context is still alive (defunct not yet set).
        // Enter/exit guards in popError() are bypassable because defunct is false.
        var errors = new ArrayList<String>();
        try {
            String err;
            while ((err = popErrorRaw()) != null) {
                errors.add(err);
            }
        } catch (Throwable t) {
            Ark.LOGGER.error("Failed to drain errors during fatal", t);
        }
        this.fatalErrors = Collections.unmodifiableList(errors);
        this.defunct = true;
    }

    /**
     * Unchecked version of {@link #popError} that skips the enter/exit guards.
     * Safe to call from {@link #markDefunct} because defunct is still {@code false}
     * at that point and the native context is still alive.
     */
    private @Nullable String popErrorRaw() {
        try {
            var errorPtr = (MemorySegment) POP_ERROR.invokeExact(this.address);
            if (MemorySegment.NULL.equals(errorPtr)) {
                return null;
            }
            var msg = errorPtr.getString(0);
            FREE_STRING.invokeExact(errorPtr);
            return msg;
        } catch (Throwable t) {
            Ark.LOGGER.error("Failed to pop error", t);
            return null;
        }
    }

    // ── Guard helpers ──────────────────────────────────────────────────────

    /**
     * Entry check: throws {@link FatalNativeException} if the context is
     * already defunct (a native panic occurred earlier).
     */
    private void enter() {
        if (this.defunct) {
            throw new FatalNativeException("Native context is defunct");
        }
    }

    /**
     * Exit check: if the context became defunct during the native call,
     * release the native resources (idempotent) and throw.
     */
    private void exit() {
        if (!this.defunct) {
            return;
        }
        this.destroy();
        throw new FatalNativeException("Native context destroyed due to fatal native error");
    }

    // ── Factory ────────────────────────────────────────────────────────────
    public static @NotNull NativeContext create(
            long instanceHandle, long deviceHandle, long vmaHandle,
            VulkanQueue graphicsQueue, VulkanQueue computeQueue, VulkanQueue transferQueue,
            Path extensionFolder
    ) {
        try (var arena = Arena.ofConfined()) {
            var pathSegment = arena.allocateFrom(extensionFolder.toAbsolutePath().toString());

            long address = (long) CREATE_NATIVE_CONTEXT.invokeExact(
                    instanceHandle, deviceHandle, vmaHandle,
                    graphicsQueue.vkQueue().address(),
                    computeQueue.vkQueue().address(),
                    transferQueue.vkQueue().address(),
                    graphicsQueue.queueFamilyIndex(),
                    computeQueue.queueFamilyIndex(),
                    transferQueue.queueFamilyIndex(),
                    pathSegment
            );

            if (address == 0) {
                Ark.LOGGER.error("ark_create_native_context returned null");
                throw new FatalNativeException("Failed to create Ark native context due to unknown error.");
            }

            return new NativeContext(address);
        } catch (Throwable t) {
            Ark.LOGGER.error("Failed to call ark_create_native_context", t);
            throw new RuntimeException(t);
        }
    }

    // ── Lifecycle ──────────────────────────────────────────────────────────

    /**
     * Releases the native context.  Idempotent — subsequent calls are no-ops.
     */
    public void destroy() {
        if (!destroyed.compareAndSet(false, true)) {
            return;
        }
        try {
            DESTROY_NATIVE_CONTEXT.invokeExact(this.address);
        } catch (Throwable t) {
            Ark.LOGGER.error("Failed to call ark_destroy_native_context", t);
        }
    }

    // ── error retrieval ────────────────────────────────────────────────────

    /// Pops the most recent error from the native context, or null if empty.
    public @Nullable String popError() {
        enter();
        try {
            var errorPtr = (MemorySegment) POP_ERROR.invokeExact(this.address);
            if (MemorySegment.NULL.equals(errorPtr)) {
                return null;
            }
            var msg = errorPtr.getString(0);
            FREE_STRING.invokeExact(errorPtr);
            return msg;
        } catch (FatalNativeException e) {
            throw e;
        } catch (Throwable t) {
            Ark.LOGGER.error("Failed to pop error from native context", t);
            return null;
        } finally {
            exit();
        }
    }

    /// Returns the number of errors pending in the native context.
    public int errorCount() {
        enter();
        try {
            return (int) ERROR_COUNT.invokeExact(this.address);
        } catch (FatalNativeException e) {
            throw e;
        } catch (Throwable t) {
            Ark.LOGGER.error("Failed to get error count from native context", t);
            return 0;
        } finally {
            exit();
        }
    }

    /// Drains all pending errors from the native context into a list.
    /// If the context is defunct, returns the errors that were collected
    /// during the fatal event.
    public List<String> drainErrors() {
        if (this.defunct) {
            return this.fatalErrors;
        }
        enter();
        try {
            var count = this.errorCount();
            if (count == 0) {
                return Collections.emptyList();
            }
            var errors = new ArrayList<String>(count);
            String err;
            while ((err = this.popError()) != null) {
                errors.add(err);
            }
            return errors;
        } finally {
            exit();
        }
    }

    // ── extension management ───────────────────────────────────────────────

    /// Loads an extension from a zip file in the extension folder.
    ///
    /// @param fileName     the zip file name (relative to the extension folder)
    /// @param wasiFeatures WASI feature strings; pass null or empty for none
    /// @throws NativeException if the native call fails
    public void loadExtension(String fileName, @Nullable List<String> wasiFeatures) {
        enter();
        try (var arena = Arena.ofConfined()) {
            var nameSeg = arena.allocateFrom(fileName);
            var jsonStr = toJsonArray(wasiFeatures);
            var jsonSeg = jsonStr != null ? arena.allocateFrom(jsonStr) : MemorySegment.NULL;
            int rc = (int) LOAD_EXTENSION.invokeExact(this.address, nameSeg, jsonSeg);
            if (rc != 0) {
                var errors = drainErrors();
                throw new NativeException("Failed to load extension '" + fileName + "'", errors.toArray(new String[0]));
            }
        } catch (NativeException e) {
            throw e;
        } catch (Throwable t) {
            var errors = drainErrors();
            throw new NativeException("Failed to load extension '" + fileName + "'", errors.toArray(new String[0]));
        } finally {
            exit();
        }
    }

    /// Initializes a specific loaded extension by its manifest id.
    ///
    /// @throws NativeException if the native call fails
    public void initializeExtension(String id) {
        enter();
        try (var arena = Arena.ofConfined()) {
            var idSeg = arena.allocateFrom(id);
            int rc = (int) INITIALIZE_EXTENSION.invokeExact(this.address, idSeg);
            if (rc != 0) {
                var errors = drainErrors();
                throw new NativeException("Failed to initialize extension '" + id + "'", errors.toArray(new String[0]));
            }
        } catch (NativeException e) {
            throw e;
        } catch (Throwable t) {
            var errors = drainErrors();
            throw new NativeException("Failed to initialize extension '" + id + "'", errors.toArray(new String[0]));
        } finally {
            exit();
        }
    }

    /// Initializes all loaded extensions.
    ///
    /// @throws NativeException if the native call fails
    public void initializeExtensions() {
        enter();
        try {
            int rc = (int) INITIALIZE_EXTENSIONS.invokeExact(this.address);
            if (rc != 0) {
                var errors = drainErrors();
                throw new NativeException("Failed to initialize extensions", errors.toArray(new String[0]));
            }
        } catch (NativeException e) {
            throw e;
        } catch (Throwable t) {
            var errors = drainErrors();
            throw new NativeException("Failed to initialize extensions", errors.toArray(new String[0]));
        } finally {
            exit();
        }
    }

    /// Disables an extension: runs its close function and removes its hooks.
    /// The extension remains loaded but inactive.
    /// @throws NativeException if the native call fails
    public void disableExtension(String id) {
        enter();
        try (var arena = Arena.ofConfined()) {
            var idSeg = arena.allocateFrom(id);
            int rc = (int) DISABLE_EXTENSION.invokeExact(this.address, idSeg);
            if (rc != 0) {
                var errors = drainErrors();
                throw new NativeException("Failed to disable extension '" + id + "'", errors.toArray(new String[0]));
            }
        } catch (NativeException e) {
            throw e;
        } catch (Throwable t) {
            var errors = drainErrors();
            throw new NativeException("Failed to disable extension '" + id + "'", errors.toArray(new String[0]));
        } finally {
            exit();
        }
    }

    /// Unloads an extension: disables it and removes it from memory.
    /// @throws NativeException if the native call fails
    public void unloadExtension(String id) {
        enter();
        try (var arena = Arena.ofConfined()) {
            var idSeg = arena.allocateFrom(id);
            int rc = (int) UNLOAD_EXTENSION.invokeExact(this.address, idSeg);
            if (rc != 0) {
                var errors = drainErrors();
                throw new NativeException("Failed to unload extension '" + id + "'", errors.toArray(new String[0]));
            }
        } catch (NativeException e) {
            throw e;
        } catch (Throwable t) {
            var errors = drainErrors();
            throw new NativeException("Failed to unload extension '" + id + "'", errors.toArray(new String[0]));
        } finally {
            exit();
        }
    }

    /// Sets the enabled Vulkan feature names on the native side, as a JSON array.
    /// This populates the sets queried by WASM extensions via check_vulkan_feature().
    /// @throws NativeException if the native call fails
    public void setEnabledVulkanFeatures(@Nullable List<String> features) {
        enter();
        try (var arena = Arena.ofConfined()) {
            var jsonStr = toJsonArray(features);
            var jsonSeg = jsonStr != null ? arena.allocateFrom(jsonStr) : MemorySegment.NULL;
            int rc = (int) SET_ENABLED_VULKAN_FEATURES.invokeExact(this.address, jsonSeg);
            if (rc != 0) {
                var errors = drainErrors();
                throw new NativeException("Failed to set enabled vulkan features", errors.toArray(new String[0]));
            }
        } catch (NativeException e) {
            throw e;
        } catch (Throwable t) {
            var errors = drainErrors();
            throw new NativeException("Failed to set enabled vulkan features", errors.toArray(new String[0]));
        } finally {
            exit();
        }
    }

    /// Sets the enabled Vulkan extension names on the native side, as a JSON array.
    /// This populates the sets queried by WASM extensions via check_vulkan_extension().
    /// @throws NativeException if the native call fails
    public void setEnabledVulkanExtensions(@Nullable List<String> extensions) {
        enter();
        try (var arena = Arena.ofConfined()) {
            var jsonStr = toJsonArray(extensions);
            var jsonSeg = jsonStr != null ? arena.allocateFrom(jsonStr) : MemorySegment.NULL;
            int rc = (int) SET_ENABLED_VULKAN_EXTENSIONS.invokeExact(this.address, jsonSeg);
            if (rc != 0) {
                var errors = drainErrors();
                throw new NativeException("Failed to set enabled vulkan extensions", errors.toArray(new String[0]));
            }
        } catch (NativeException e) {
            throw e;
        } catch (Throwable t) {
            var errors = drainErrors();
            throw new NativeException("Failed to set enabled vulkan extensions", errors.toArray(new String[0]));
        } finally {
            exit();
        }
    }

    public long getAddress() {
        return this.address;
    }

    @Override
    public String toString() {
        return Long.toHexString(this.getAddress());
    }

    // ── helpers ────────────────────────────────────────────────────────────

    private static @Nullable String toJsonArray(@Nullable List<String> items) {
        if (items == null || items.isEmpty()) {
            return null;
        }
        var sb = new StringBuilder("[");
        for (int i = 0; i < items.size(); i++) {
            if (i > 0) sb.append(',');
            sb.append('"');
            sb.append(items.get(i).replace("\\", "\\\\").replace("\"", "\\\""));
            sb.append('"');
        }
        sb.append(']');
        return sb.toString();
    }
}

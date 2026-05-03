package io.github.liyze09.ark;

import org.jetbrains.annotations.Contract;
import org.jspecify.annotations.NonNull;

import java.lang.foreign.Arena;
import java.lang.foreign.FunctionDescriptor;
import java.lang.foreign.Linker;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.SymbolLookup;
import java.lang.foreign.ValueLayout;
import java.lang.invoke.MethodHandle;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

public final class NativeContext {
    private static final MethodHandle CREATE_NATIVE_CONTEXT;
    private static final MethodHandle DESTROY_NATIVE_CONTEXT;
    private static final MethodHandle POP_ERROR;
    private static final MethodHandle ERROR_COUNT;
    private static final MethodHandle FREE_STRING;

    private final long address;

    private NativeContext(long address) {
        this.address = address;
    }

    static {
        try {
            System.loadLibrary("ark");
            var linker = Linker.nativeLinker();
            var lookup = SymbolLookup.loaderLookup();

            var createSymbol = lookup.find("ark_create_native_context").orElseThrow();
            CREATE_NATIVE_CONTEXT = linker.downcallHandle(
                createSymbol,
                FunctionDescriptor.of(
                    ValueLayout.JAVA_LONG,
                    ValueLayout.JAVA_LONG, ValueLayout.JAVA_LONG,
                    ValueLayout.JAVA_LONG, ValueLayout.JAVA_LONG,
                    ValueLayout.JAVA_LONG, ValueLayout.JAVA_LONG,
                    ValueLayout.ADDRESS
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

            var freeStringSymbol = lookup.find("ark_free_string").orElseThrow();
            FREE_STRING = linker.downcallHandle(
                freeStringSymbol,
                FunctionDescriptor.ofVoid(ValueLayout.ADDRESS)
            );
        } catch (Exception e) {
            throw new ExceptionInInitializerError(e);
        }
    }

    @Contract("_, _, _, _, _, _, _ -> new")
    public static @NonNull NativeContext create(
        long instanceHandle, long deviceHandle, long vmaHandle,
        long transferQueue, long graphicsQueue, long computeQueue,
        Path extensionFolder
    ) {
        try (var arena = Arena.ofConfined()) {
            var pathSegment = arena.allocateFrom(extensionFolder.toAbsolutePath().toString());
            return new NativeContext((long) CREATE_NATIVE_CONTEXT.invokeExact(
                instanceHandle, deviceHandle, vmaHandle,
                transferQueue, graphicsQueue, computeQueue,
                pathSegment
            ));
        } catch (Throwable t) {
            Ark.LOGGER.error("Failed to call ark_create_native_context", t);
            throw new RuntimeException(t);
        }
    }

    public void destroy() {
        try {
            DESTROY_NATIVE_CONTEXT.invokeExact(this.address);
        } catch (Throwable t) {
            Ark.LOGGER.error("Failed to call ark_destroy_native_context", t);
        }
    }

    // ── error retrieval ────────────────────────────────────────

    /// Pops the most recent error from the native context, or null if empty.
    public String popError() {
        try {
            var errorPtr = (MemorySegment) POP_ERROR.invokeExact(this.address);
            if (MemorySegment.NULL.equals(errorPtr)) {
                return null;
            }
            var msg = errorPtr.getString(0);
            FREE_STRING.invokeExact(errorPtr);
            return msg;
        } catch (Throwable t) {
            Ark.LOGGER.error("Failed to pop error from native context", t);
            return null;
        }
    }

    /// Returns the number of errors pending in the native context.
    public int errorCount() {
        try {
            return (int) ERROR_COUNT.invokeExact(this.address);
        } catch (Throwable t) {
            Ark.LOGGER.error("Failed to get error count from native context", t);
            return 0;
        }
    }

    /// Drains all pending errors from the native context into a list.
    public List<String> drainErrors() {
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
    }

    public long getAddress() {
        return this.address;
    }

    @Override
    public String toString() {
        return Long.toHexString(this.getAddress());
    }
}

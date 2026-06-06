package io.github.liyze09.ark.exception;

/**
 * Thrown when a native (Rust) panic has been caught and the {@code NativeContext}
 * has been marked defunct.  All subsequent calls into the same context will fail
 * with this exception, and the first call that returns after the fatal event
 * automatically releases the native resources.
 */
public class FatalNativeException extends RuntimeException {
    public FatalNativeException(String message) {
        super(message);
    }
}

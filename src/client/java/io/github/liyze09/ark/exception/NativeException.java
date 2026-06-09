package io.github.liyze09.ark.exception;

public class NativeException extends RuntimeException {
    public final String[] errors;

    public NativeException(String message) {
        super(message);
        this.errors = new String[0];
    }

    public NativeException(String[] errors) {
        super();
        this.errors = errors;
    }

    public NativeException(String message, String[] errors) {
        super(message);
        this.errors = errors;
    }

    @Override
    public String getMessage() {
        return super.getMessage()
                + (errors.length > 0 ? ":" + String.join(", ", errors) : "");
    }
}

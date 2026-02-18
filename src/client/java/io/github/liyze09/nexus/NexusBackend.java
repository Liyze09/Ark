package io.github.liyze09.nexus;

import io.github.liyze09.nexus.resource.SharedTextureManager;

import java.io.Closeable;
import java.lang.foreign.MemorySegment;

/**
 * Instantiating class that wraps Nexus JNI methods.
 * Manages the lifecycle of native Vulkan context.
 */
public class NexusBackend implements Closeable {
    private final long nativeContext;
    public final SharedTextureManager sharedTextureManager = new SharedTextureManager();
    private boolean closed = false;

    /**
     * Initializes a new Nexus backend instance.
     * Calls the native initialization function.
     */
    public NexusBackend() {
        this.nativeContext = NexusClientMain.initNative();
    }

    /**
     * Queues rendering operation.
     */
    public void render() {
        checkClosed();
        NexusClientMain.render(nativeContext);
    }

    /**
     * Resizes the render target.
     *
     * @param width  New width
     * @param height New height
     */
    public void resize(int width, int height) {
        checkClosed();
        NexusClientMain.resize(nativeContext, width, height);
    }

    /**
     * Gets current render target's size.
     */
    public long getRenderTargetSize() {
        checkClosed();
        return NexusClientMain.getTextureSize(nativeContext);
    }

    /**
     * Gets GL ready semaphore handle.
     */
    public long getGLReady() {
        checkClosed();
        return NexusClientMain.getGLReady(nativeContext);
    }

    /**
     * Gets GL complete semaphore handle.
     */
    public long getGLComplete() {
        checkClosed();
        return NexusClientMain.getGLComplete(nativeContext);
    }

    /**
     * Gets render target's external texture handle.
     */
    public long getGLTexture() {
        checkClosed();
        return NexusClientMain.getGLTexture(nativeContext);
    }

    /**
     * Gets Vulkan-handled texture's size.
     *
     * @param handle Vulkan texture handle
     * @return Texture size
     */
    public long getVulkanTextureSize(long handle) {
        checkClosed();
        return NexusClientMain.getVulkanTextureSize(nativeContext, handle);
    }

    /**
     * Creates an external Vulkan texture.
     *
     * @param width     Width
     * @param height    Height
     * @param mipLevels MIP levels
     * @return Vulkan texture handle
     */
    public long acquireVulkanTexture(int width, int height, int mipLevels) {
        checkClosed();
        return NexusClientMain.acquireVulkanTexture(nativeContext, width, height, mipLevels);
    }

    /**
     * Closes and releases native resources.
     */
    @Override
    public void close() {
        if (!closed) {
            NexusClientMain.close(nativeContext);
            closed = true;
        }
    }

    /**
     * Checks if the instance is closed.
     *
     * @throws IllegalStateException if the instance is closed
     */
    private void checkClosed() {
        if (closed) {
            throw new IllegalStateException("NexusBackend instance is closed");
        }
    }

    public void syncTerrainData(long header, MemorySegment data) {
        checkClosed();
        NexusClientMain.syncTerrainData(nativeContext, header, data.address(), data.byteSize());
    }

    public void syncAtlas(
        long textureHandle,
        String atlasName,
        String[] spriteNames,
        int[] spriteX, int[] spriteY,
        int[] spriteWidth, int[] spriteHeight,
        float[] spriteU0, float[] spriteV0,
        float[] spriteU1, float[] spriteV1
    ) {
        checkClosed();
        NexusClientMain.syncAtlas(
            nativeContext,
            textureHandle,
            atlasName,
            spriteNames,
            spriteX, spriteY,
            spriteWidth, spriteHeight,
            spriteU0, spriteV0,
            spriteU1, spriteV1
        );
    }

    /**
     * Gets the underlying native context handle.
     */
    public long getNativeContext() {
        return nativeContext;
    }

    /**
     * Checks if the instance is closed.
     *
     * @return true if closed
     */
    public boolean isClosed() {
        return closed;
    }
}
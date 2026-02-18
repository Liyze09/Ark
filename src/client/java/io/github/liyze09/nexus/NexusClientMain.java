package io.github.liyze09.nexus;

import io.github.liyze09.nexus.render.NexusWorldRenderer;
import net.fabricmc.api.ClientModInitializer;
import net.minecraft.client.Minecraft;

public class NexusClientMain implements ClientModInitializer {
    public static Configuration config = new Configuration();

    public static NexusWorldRenderer getNexusRenderer() {
        if (Minecraft.getInstance().levelRenderer instanceof NexusWorldRenderer renderer)
            return renderer;
        else {
            throw new IllegalStateException("NexusWorldRenderer is not initialized");
        }
    }

    static {
        System.loadLibrary("nexus");
    }

    static native long initNative();

    static native void render(long ctx);

    static native void close(long ctx);

    static native long getTextureSize(long ctx);

    static native void resize(long ctx, int width, int height);

    static native long getGLReady(long ctx);

    static native long getGLComplete(long ctx);

    static native long getGLTexture(long ctx);

    static native long acquireVulkanTexture(long ctx, int width, int height, int mipLevels);

    static native long getVulkanTextureSize(long ctx, long handle);

    static native void syncTerrainData(long ctx, long header, long dataAddress, long dataSize);

    static native void syncAtlas(long ctx, long textureHandle, String atlasName,
                                        String[] spriteNames,
                                        int[] spriteX, int[] spriteY,
                                        int[] spriteWidth, int[] spriteHeight,
                                        float[] spriteU0, float[] spriteV0,
                                        float[] spriteU1, float[] spriteV1);

    @Override
    public void onInitializeClient() {

    }
}
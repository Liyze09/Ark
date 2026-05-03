package io.github.liyze09.ark;

import com.mojang.blaze3d.systems.RenderSystem;
import com.mojang.blaze3d.vulkan.VulkanDevice;
import io.github.liyze09.ark.extension.ExtensionLoader;
import io.github.liyze09.ark.mixin.GpuDeviceAccessor;
import net.fabricmc.api.ClientModInitializer;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

public class Ark implements ClientModInitializer {
    public static final Logger LOGGER = LoggerFactory.getLogger(Ark.class);
    private static ExtensionLoader loader;

    public static ExtensionLoader getExtensionLoader() {
        if (loader == null) {
            loader = new ExtensionLoader();
        }
        return loader;
    }

    private static NativeContext nativeContext;

    public static NativeContext getNativeContext() {
        return nativeContext;
    }

    @Override
    public void onInitializeClient() {
        var device = RenderSystem.getDevice();
        if (!device.getDeviceInfo().backendName().equalsIgnoreCase("Vulkan")) {
            LOGGER.error("Fatal error: Ark only support Vulkan Minecraft");
            throw new IllegalStateException("Ark only support Vulkan Minecraft");
        }
        LOGGER.info("Ark running on {}", device.getDeviceInfo().name());
        var backend = (VulkanDevice) ((GpuDeviceAccessor) device).getGpuDeviceBackend();
        var vkInstance = backend.instance().vkInstance().address();
        var vkDevice = backend.vkDevice().address();
        var vma = backend.vma();
        var graphicsQueue = backend.graphicsQueue().vkQueue().address();
        var computeQueue = backend.computeQueue().vkQueue().address();
        var transferQueue = backend.transferQueue().vkQueue().address();

        nativeContext = NativeContext.create(
            vkInstance, vkDevice, vma,
            transferQueue, graphicsQueue, computeQueue,
            ExtensionLoader.extensionPath
        );

        LOGGER.info("Native context created: 0x{}", nativeContext);
    }
}

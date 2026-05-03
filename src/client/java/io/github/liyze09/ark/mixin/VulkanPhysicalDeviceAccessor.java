package io.github.liyze09.ark.mixin;

import com.mojang.blaze3d.vulkan.VulkanPhysicalDevice;
import org.lwjgl.vulkan.VkPhysicalDeviceFeatures2;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

@Mixin(VulkanPhysicalDevice.class)
public interface VulkanPhysicalDeviceAccessor {
    @Accessor("vkPhysicalDeviceFeatures")
    VkPhysicalDeviceFeatures2 getDeviceFeatures();
}

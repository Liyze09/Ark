package io.github.liyze09.ark.mixin;

import com.llamalad7.mixinextras.sugar.Local;
import com.mojang.blaze3d.shaders.GpuDebugOptions;
import com.mojang.blaze3d.shaders.ShaderSource;
import com.mojang.blaze3d.systems.GpuDevice;
import com.mojang.blaze3d.vulkan.VulkanBackend;
import com.mojang.blaze3d.vulkan.VulkanPhysicalDevice;
import com.mojang.blaze3d.vulkan.init.VulkanFeature;
import io.github.liyze09.ark.Ark;
import org.lwjgl.system.MemoryStack;
import org.lwjgl.vulkan.VkPhysicalDeviceFeatures2;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.Redirect;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

import java.util.Set;

@Mixin(VulkanBackend.class)
public class VulkanBackendMixin {
    @Inject(
           method = "createDevice(JLcom/mojang/blaze3d/shaders/ShaderSource;Lcom/mojang/blaze3d/shaders/GpuDebugOptions;)Lcom/mojang/blaze3d/systems/GpuDevice;",
            at = @At(
                    value = "INVOKE",
                    target = "Lcom/mojang/blaze3d/vulkan/VulkanBackend;createDevice(Ljava/util/Collection;Lcom/mojang/blaze3d/vulkan/VulkanPhysicalDevice;)Lorg/lwjgl/vulkan/VkDevice;"
            )
    )
    public void addVulkanExtensions(long window, ShaderSource defaultShaderSource, GpuDebugOptions debugOptions, CallbackInfoReturnable<GpuDevice> cir, @Local(name = "deviceExtensions") Set<String> deviceExtensions, @Local(name = "physicalDevice") VulkanPhysicalDevice physicalDevice) {
        var loader = Ark.getExtensionLoader();
        loader.checkCompatibility(physicalDevice.vkPhysicalDevice(), ((VulkanPhysicalDeviceAccessor) physicalDevice).getDeviceFeatures());
        deviceExtensions.addAll(loader.getNeededVulkanExtensions());
    }

    @Redirect(
            method = "createDevice(Ljava/util/Collection;Lcom/mojang/blaze3d/vulkan/VulkanPhysicalDevice;)Lorg/lwjgl/vulkan/VkDevice;",
            at = @At(
                    value = "INVOKE",
                    target = "Lorg/lwjgl/vulkan/VkPhysicalDeviceFeatures2;sType$Default()Lorg/lwjgl/vulkan/VkPhysicalDeviceFeatures2;"
            )
    )
    private static VkPhysicalDeviceFeatures2 addVulkanFeatures(VkPhysicalDeviceFeatures2 instance,
                                                               @Local(name = "stack") MemoryStack stack
    ) {
        var features = instance.sType$Default();
        var required = Ark.getExtensionLoader().getNeededVulkanFeatures();
        for (VulkanFeature requiredDeviceFeature : required) {
            requiredDeviceFeature.set(features, true, stack);
        }
        return features;
    }
}

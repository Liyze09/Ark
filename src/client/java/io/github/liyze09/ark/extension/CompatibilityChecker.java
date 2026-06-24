package io.github.liyze09.ark.extension;

import com.mojang.blaze3d.vulkan.init.VulkanFeature;
import com.mojang.blaze3d.vulkan.init.VulkanPNextStruct;
import io.github.liyze09.ark.Ark;
import org.jspecify.annotations.Nullable;
import org.lwjgl.system.MemoryStack;
import org.lwjgl.vulkan.*;

import java.lang.reflect.Field;
import java.lang.reflect.Modifier;
import java.util.*;

public class CompatibilityChecker {

    // ── struct class → pNext descriptor ────────────────────────────────

    /**
     * Special sType marker recognized by VulkanFeature for embedded VkPhysicalDeviceFeatures.
     */
    static final int VK10_STYPE = 1000059000;
    static final Map<String, VulkanFeature> KNOWN_FEATURES;
    private static final VulkanPNextStruct VK10_STRUCT = new VulkanPNextStruct(VK10_STYPE, VkPhysicalDeviceProperties2.SIZEOF);
    private static final Map<Class<?>, VulkanPNextStruct> STRUCT_DESCRIPTORS;
    /**
     * All VkPhysicalDevice*Feature* classes from VkPhysicalDeviceFeatures2.java lines 92–713.
     */
    private static final String[] P_NEXT_STRUCT_NAMES = {
            "VkPhysicalDevice16BitStorageFeatures",
            "VkPhysicalDevice4444FormatsFeaturesEXT",
            "VkPhysicalDevice8BitStorageFeatures",
            "VkPhysicalDeviceAccelerationStructureFeaturesKHR",
            "VkPhysicalDeviceAddressBindingReportFeaturesEXT",
            "VkPhysicalDeviceAmigoProfilingFeaturesSEC",
            "VkPhysicalDeviceAntiLagFeaturesAMD",
            "VkPhysicalDeviceASTCDecodeFeaturesEXT",
            "VkPhysicalDeviceAttachmentFeedbackLoopDynamicStateFeaturesEXT",
            "VkPhysicalDeviceAttachmentFeedbackLoopLayoutFeaturesEXT",
            "VkPhysicalDeviceBlendOperationAdvancedFeaturesEXT",
            "VkPhysicalDeviceBorderColorSwizzleFeaturesEXT",
            "VkPhysicalDeviceBufferDeviceAddressFeatures",
            "VkPhysicalDeviceClusterAccelerationStructureFeaturesNV",
            "VkPhysicalDeviceClusterCullingShaderFeaturesHUAWEI",
            "VkPhysicalDeviceCoherentMemoryFeaturesAMD",
            "VkPhysicalDeviceColorWriteEnableFeaturesEXT",
            "VkPhysicalDeviceCommandBufferInheritanceFeaturesNV",
            "VkPhysicalDeviceComputeOccupancyPriorityFeaturesNV",
            "VkPhysicalDeviceComputeShaderDerivativesFeaturesKHR",
            "VkPhysicalDeviceConditionalRenderingFeaturesEXT",
            "VkPhysicalDeviceCooperativeMatrix2FeaturesNV",
            "VkPhysicalDeviceCooperativeMatrixConversionFeaturesQCOM",
            "VkPhysicalDeviceCooperativeMatrixFeaturesKHR",
            "VkPhysicalDeviceCooperativeVectorFeaturesNV",
            "VkPhysicalDeviceCopyMemoryIndirectFeaturesKHR",
            "VkPhysicalDeviceCornerSampledImageFeaturesNV",
            "VkPhysicalDeviceCoverageReductionModeFeaturesNV",
            "VkPhysicalDeviceCubicClampFeaturesQCOM",
            "VkPhysicalDeviceCubicWeightsFeaturesQCOM",
            "VkPhysicalDeviceCudaKernelLaunchFeaturesNV",
            "VkPhysicalDeviceCustomBorderColorFeaturesEXT",
            "VkPhysicalDeviceCustomResolveFeaturesEXT",
            "VkPhysicalDeviceDataGraphFeaturesARM",
            "VkPhysicalDeviceDataGraphModelFeaturesQCOM",
            "VkPhysicalDeviceDedicatedAllocationImageAliasingFeaturesNV",
            "VkPhysicalDeviceDenseGeometryFormatFeaturesAMDX",
            "VkPhysicalDeviceDepthBiasControlFeaturesEXT",
            "VkPhysicalDeviceDepthClampControlFeaturesEXT",
            "VkPhysicalDeviceDepthClampZeroOneFeaturesEXT",
            "VkPhysicalDeviceDepthClipControlFeaturesEXT",
            "VkPhysicalDeviceDepthClipEnableFeaturesEXT",
            "VkPhysicalDeviceDescriptorBufferFeaturesEXT",
            "VkPhysicalDeviceDescriptorBufferTensorFeaturesARM",
            "VkPhysicalDeviceDescriptorHeapFeaturesEXT",
            "VkPhysicalDeviceDescriptorIndexingFeatures",
            "VkPhysicalDeviceDescriptorPoolOverallocationFeaturesNV",
            "VkPhysicalDeviceDescriptorSetHostMappingFeaturesVALVE",
            "VkPhysicalDeviceDeviceGeneratedCommandsComputeFeaturesNV",
            "VkPhysicalDeviceDeviceGeneratedCommandsFeaturesEXT",
            "VkPhysicalDeviceDeviceMemoryReportFeaturesEXT",
            "VkPhysicalDeviceDiagnosticsConfigFeaturesNV",
            "VkPhysicalDeviceDisplacementMicromapFeaturesNV",
            "VkPhysicalDeviceDynamicRenderingFeatures",
            "VkPhysicalDeviceDynamicRenderingLocalReadFeatures",
            "VkPhysicalDeviceDynamicRenderingUnusedAttachmentsFeaturesEXT",
            "VkPhysicalDeviceExclusiveScissorFeaturesNV",
            "VkPhysicalDeviceExtendedDynamicState2FeaturesEXT",
            "VkPhysicalDeviceExtendedDynamicState3FeaturesEXT",
            "VkPhysicalDeviceExtendedDynamicStateFeaturesEXT",
            "VkPhysicalDeviceExtendedSparseAddressSpaceFeaturesNV",
            "VkPhysicalDeviceExternalFormatResolveFeaturesANDROID",
            "VkPhysicalDeviceExternalMemoryRDMAFeaturesNV",
            "VkPhysicalDeviceFaultFeaturesEXT",
            "VkPhysicalDeviceFloat16Int8FeaturesKHR",
            "VkPhysicalDeviceFormatPackFeaturesARM",
            "VkPhysicalDeviceFragmentDensityMap2FeaturesEXT",
            "VkPhysicalDeviceFragmentDensityMapFeaturesEXT",
            "VkPhysicalDeviceFragmentDensityMapLayeredFeaturesVALVE",
            "VkPhysicalDeviceFragmentDensityMapOffsetFeaturesEXT",
            "VkPhysicalDeviceFragmentShaderBarycentricFeaturesKHR",
            "VkPhysicalDeviceFragmentShaderInterlockFeaturesEXT",
            "VkPhysicalDeviceFragmentShadingRateEnumsFeaturesNV",
            "VkPhysicalDeviceFragmentShadingRateFeaturesKHR",
            "VkPhysicalDeviceFrameBoundaryFeaturesEXT",
            "VkPhysicalDeviceGlobalPriorityQueryFeatures",
            "VkPhysicalDeviceGraphicsPipelineLibraryFeaturesEXT",
            "VkPhysicalDeviceHdrVividFeaturesHUAWEI",
            "VkPhysicalDeviceHostImageCopyFeatures",
            "VkPhysicalDeviceHostQueryResetFeatures",
            "VkPhysicalDeviceImage2DViewOf3DFeaturesEXT",
            "VkPhysicalDeviceImageAlignmentControlFeaturesMESA",
            "VkPhysicalDeviceImageCompressionControlFeaturesEXT",
            "VkPhysicalDeviceImageCompressionControlSwapchainFeaturesEXT",
            "VkPhysicalDeviceImagelessFramebufferFeatures",
            "VkPhysicalDeviceImageProcessing2FeaturesQCOM",
            "VkPhysicalDeviceImageProcessingFeaturesQCOM",
            "VkPhysicalDeviceImageRobustnessFeatures",
            "VkPhysicalDeviceImageSlicedViewOf3DFeaturesEXT",
            "VkPhysicalDeviceImageViewMinLodFeaturesEXT",
            "VkPhysicalDeviceIndexTypeUint8Features",
            "VkPhysicalDeviceInheritedViewportScissorFeaturesNV",
            "VkPhysicalDeviceInlineUniformBlockFeatures",
            "VkPhysicalDeviceInternallySynchronizedQueuesFeaturesKHR",
            "VkPhysicalDeviceInvocationMaskFeaturesHUAWEI",
            "VkPhysicalDeviceLegacyDitheringFeaturesEXT",
            "VkPhysicalDeviceLegacyVertexAttributesFeaturesEXT",
            "VkPhysicalDeviceLinearColorAttachmentFeaturesNV",
            "VkPhysicalDeviceLineRasterizationFeatures",
            "VkPhysicalDeviceMaintenance4Features",
            "VkPhysicalDeviceMaintenance5Features",
            "VkPhysicalDeviceMaintenance6Features",
            "VkPhysicalDeviceMaintenance7FeaturesKHR",
            "VkPhysicalDeviceMaintenance8FeaturesKHR",
            "VkPhysicalDeviceMaintenance9FeaturesKHR",
            "VkPhysicalDeviceMapMemoryPlacedFeaturesEXT",
            "VkPhysicalDeviceMemoryDecompressionFeaturesEXT",
            "VkPhysicalDeviceMemoryPriorityFeaturesEXT",
            "VkPhysicalDeviceMeshShaderFeaturesEXT",
            "VkPhysicalDeviceMultiDrawFeaturesEXT",
            "VkPhysicalDeviceMultisampledRenderToSingleSampledFeaturesEXT",
            "VkPhysicalDeviceMultiviewFeatures",
            "VkPhysicalDeviceMultiviewPerViewRenderAreasFeaturesQCOM",
            "VkPhysicalDeviceMultiviewPerViewViewportsFeaturesQCOM",
            "VkPhysicalDeviceMutableDescriptorTypeFeaturesEXT",
            "VkPhysicalDeviceNestedCommandBufferFeaturesEXT",
            "VkPhysicalDeviceNonSeamlessCubeMapFeaturesEXT",
            "VkPhysicalDeviceOpacityMicromapFeaturesEXT",
            "VkPhysicalDeviceOpticalFlowFeaturesNV",
            "VkPhysicalDevicePageableDeviceLocalMemoryFeaturesEXT",
            "VkPhysicalDevicePartitionedAccelerationStructureFeaturesNV",
            "VkPhysicalDevicePerformanceCountersByRegionFeaturesARM",
            "VkPhysicalDevicePerformanceQueryFeaturesKHR",
            "VkPhysicalDevicePerStageDescriptorSetFeaturesNV",
            "VkPhysicalDevicePipelineBinaryFeaturesKHR",
            "VkPhysicalDevicePipelineCacheIncrementalModeFeaturesSEC",
            "VkPhysicalDevicePipelineCreationCacheControlFeatures",
            "VkPhysicalDevicePipelineExecutablePropertiesFeaturesKHR",
            "VkPhysicalDevicePipelineLibraryGroupHandlesFeaturesEXT",
            "VkPhysicalDevicePipelineOpacityMicromapFeaturesARM",
            "VkPhysicalDevicePipelinePropertiesFeaturesEXT",
            "VkPhysicalDevicePipelineProtectedAccessFeatures",
            "VkPhysicalDevicePipelineRobustnessFeatures",
            "VkPhysicalDevicePortabilitySubsetFeaturesKHR",
            "VkPhysicalDevicePresentBarrierFeaturesNV",
            "VkPhysicalDevicePresentIdFeaturesKHR",
            "VkPhysicalDevicePresentMeteringFeaturesNV",
            "VkPhysicalDevicePresentModeFifoLatestReadyFeaturesEXT",
            "VkPhysicalDevicePresentTimingFeaturesEXT",
            "VkPhysicalDevicePresentWait2FeaturesKHR",
            "VkPhysicalDevicePresentWaitFeaturesKHR",
            "VkPhysicalDevicePrimitivesGeneratedQueryFeaturesEXT",
            "VkPhysicalDevicePrimitiveTopologyListRestartFeaturesEXT",
            "VkPhysicalDevicePrivateDataFeatures",
            "VkPhysicalDeviceProtectedMemoryFeatures",
            "VkPhysicalDeviceProvokingVertexFeaturesEXT",
            "VkPhysicalDevicePushConstantBankFeaturesNV",
            "VkPhysicalDeviceRasterizationOrderAttachmentAccessFeaturesARM",
            "VkPhysicalDeviceRawAccessChainsFeaturesNV",
            "VkPhysicalDeviceRayQueryFeaturesKHR",
            "VkPhysicalDeviceRayTracingInvocationReorderFeaturesEXT",
            "VkPhysicalDeviceRayTracingLinearSweptSpheresFeaturesNV",
            "VkPhysicalDeviceRayTracingMaintenance1FeaturesKHR",
            "VkPhysicalDeviceRayTracingMotionBlurFeaturesNV",
            "VkPhysicalDeviceRayTracingPipelineFeaturesKHR",
            "VkPhysicalDeviceRayTracingPositionFetchFeaturesKHR",
            "VkPhysicalDeviceRayTracingValidationFeaturesNV",
            "VkPhysicalDeviceRelaxedLineRasterizationFeaturesIMG",
            "VkPhysicalDeviceRenderPassStripedFeaturesARM",
            "VkPhysicalDeviceRepresentativeFragmentTestFeaturesNV",
            "VkPhysicalDeviceRGBA10X6FormatsFeaturesEXT",
            "VkPhysicalDeviceRobustness2FeaturesEXT",
            "VkPhysicalDeviceSamplerYcbcrConversionFeatures",
            "VkPhysicalDeviceScalarBlockLayoutFeatures",
            "VkPhysicalDeviceSchedulingControlsFeaturesARM",
            "VkPhysicalDeviceSeparateDepthStencilLayoutsFeatures",
            "VkPhysicalDeviceShader64BitIndexingFeaturesEXT",
            "VkPhysicalDeviceShaderAtomicFloat16VectorFeaturesNV",
            "VkPhysicalDeviceShaderAtomicFloat2FeaturesEXT",
            "VkPhysicalDeviceShaderAtomicFloatFeaturesEXT",
            "VkPhysicalDeviceShaderAtomicInt64Features",
            "VkPhysicalDeviceShaderBfloat16FeaturesKHR",
            "VkPhysicalDeviceShaderClockFeaturesKHR",
            "VkPhysicalDeviceShaderCoreBuiltinsFeaturesARM",
            "VkPhysicalDeviceShaderDemoteToHelperInvocationFeatures",
            "VkPhysicalDeviceShaderDrawParametersFeatures",
            "VkPhysicalDeviceShaderEarlyAndLateFragmentTestsFeaturesAMD",
            "VkPhysicalDeviceShaderEnqueueFeaturesAMDX",
            "VkPhysicalDeviceShaderExpectAssumeFeatures",
            "VkPhysicalDeviceShaderFloat16Int8Features",
            "VkPhysicalDeviceShaderFloat8FeaturesEXT",
            "VkPhysicalDeviceShaderFloatControls2Features",
            "VkPhysicalDeviceShaderFmaFeaturesKHR",
            "VkPhysicalDeviceShaderImageAtomicInt64FeaturesEXT",
            "VkPhysicalDeviceShaderImageFootprintFeaturesNV",
            "VkPhysicalDeviceShaderIntegerDotProductFeatures",
            "VkPhysicalDeviceShaderIntegerFunctions2FeaturesINTEL",
            "VkPhysicalDeviceShaderLongVectorFeaturesEXT",
            "VkPhysicalDeviceShaderMaximalReconvergenceFeaturesKHR",
            "VkPhysicalDeviceShaderModuleIdentifierFeaturesEXT",
            "VkPhysicalDeviceShaderObjectFeaturesEXT",
            "VkPhysicalDeviceShaderQuadControlFeaturesKHR",
            "VkPhysicalDeviceShaderRelaxedExtendedInstructionFeaturesKHR",
            "VkPhysicalDeviceShaderReplicatedCompositesFeaturesEXT",
            "VkPhysicalDeviceShaderSMBuiltinsFeaturesNV",
            "VkPhysicalDeviceShaderSubgroupExtendedTypesFeatures",
            "VkPhysicalDeviceShaderSubgroupPartitionedFeaturesEXT",
            "VkPhysicalDeviceShaderSubgroupRotateFeatures",
            "VkPhysicalDeviceShaderSubgroupUniformControlFlowFeaturesKHR",
            "VkPhysicalDeviceShaderTerminateInvocationFeatures",
            "VkPhysicalDeviceShaderTileImageFeaturesEXT",
            "VkPhysicalDeviceShaderUniformBufferUnsizedArrayFeaturesEXT",
            "VkPhysicalDeviceShaderUntypedPointersFeaturesKHR",
            "VkPhysicalDeviceShadingRateImageFeaturesNV",
            "VkPhysicalDeviceSubgroupSizeControlFeatures",
            "VkPhysicalDeviceSubpassMergeFeedbackFeaturesEXT",
            "VkPhysicalDeviceSubpassShadingFeaturesHUAWEI",
            "VkPhysicalDeviceSwapchainMaintenance1FeaturesEXT",
            "VkPhysicalDeviceSynchronization2Features",
            "VkPhysicalDeviceTensorFeaturesARM",
            "VkPhysicalDeviceTexelBufferAlignmentFeaturesEXT",
            "VkPhysicalDeviceTextureCompressionASTC3DFeaturesEXT",
            "VkPhysicalDeviceTextureCompressionASTCHDRFeatures",
            "VkPhysicalDeviceTileMemoryHeapFeaturesQCOM",
            "VkPhysicalDeviceTilePropertiesFeaturesQCOM",
            "VkPhysicalDeviceTileShadingFeaturesQCOM",
            "VkPhysicalDeviceTimelineSemaphoreFeatures",
            "VkPhysicalDeviceTransformFeedbackFeaturesEXT",
            "VkPhysicalDeviceUnifiedImageLayoutsFeaturesKHR",
            "VkPhysicalDeviceUniformBufferStandardLayoutFeatures",
            "VkPhysicalDeviceVariablePointersFeatures",
            "VkPhysicalDeviceVertexAttributeDivisorFeatures",
            "VkPhysicalDeviceVertexAttributeRobustnessFeaturesEXT",
            "VkPhysicalDeviceVertexInputDynamicStateFeaturesEXT",
            "VkPhysicalDeviceVideoDecodeVP9FeaturesKHR",
            "VkPhysicalDeviceVideoEncodeAV1FeaturesKHR",
            "VkPhysicalDeviceVideoEncodeIntraRefreshFeaturesKHR",
            "VkPhysicalDeviceVideoEncodeQuantizationMapFeaturesKHR",
            "VkPhysicalDeviceVideoEncodeRgbConversionFeaturesVALVE",
            "VkPhysicalDeviceVideoMaintenance1FeaturesKHR",
            "VkPhysicalDeviceVideoMaintenance2FeaturesKHR",
            "VkPhysicalDeviceVulkan11Features",
            "VkPhysicalDeviceVulkan12Features",
            "VkPhysicalDeviceVulkan13Features",
            "VkPhysicalDeviceVulkan14Features",
            "VkPhysicalDeviceVulkanMemoryModelFeatures",
            "VkPhysicalDeviceWorkgroupMemoryExplicitLayoutFeaturesKHR",
            "VkPhysicalDeviceYcbcr2Plane444FormatsFeaturesEXT",
            "VkPhysicalDeviceYcbcrDegammaFeaturesQCOM",
            "VkPhysicalDeviceYcbcrImageArraysFeaturesEXT",
            "VkPhysicalDeviceZeroInitializeDeviceMemoryFeaturesEXT",
            "VkPhysicalDeviceZeroInitializeWorkgroupMemoryFeatures",
    };

    static {
        var structMap = new LinkedHashMap<Class<?>, VulkanPNextStruct>();
        var featureMap = new HashMap<String, VulkanFeature>();

        // VK10 base features are embedded, handled separately
        structMap.put(VkPhysicalDeviceFeatures.class, VK10_STRUCT);
        registerStruct(featureMap, VK10_STRUCT, VkPhysicalDeviceFeatures.class);

        // All pNext feature structs appearing in VkPhysicalDeviceFeatures2 (lines 92-713)
        for (var className : P_NEXT_STRUCT_NAMES) {
            try {
                var clazz = Class.forName("org.lwjgl.vulkan." + className);
                int sType = extractSType(clazz);
                if (sType < 0) continue;
                // Deduplicate by sType: keep the first (typically the core/promoted) version
                boolean isDuplicate = structMap.values().stream().anyMatch(d -> d.sType() == sType);
                if (isDuplicate) continue;

                int structSize = clazz.getField("SIZEOF").getInt(null);
                var descriptor = new VulkanPNextStruct(sType, structSize);
                structMap.put(clazz, descriptor);
                registerStruct(featureMap, descriptor, clazz);
            } catch (Exception e) {
                Ark.LOGGER.warn("Failed to register Vulkan feature struct '{}': {}", className, e.toString());
            }
        }

        STRUCT_DESCRIPTORS = Collections.unmodifiableMap(structMap);
        KNOWN_FEATURES = Collections.unmodifiableMap(featureMap);
    }

    // ── compatibility check ────────────────────────────────────────────

    /**
     * Extracts the sType value by allocating a temporary struct, calling sType$Default, and reading sType.
     */
    private static int extractSType(Class<?> clazz) {
        try {
            var calloc = clazz.getMethod("calloc");
            var instance = calloc.invoke(null);
            try {
                clazz.getMethod("sType$Default").invoke(instance);
                return (int) clazz.getMethod("sType").invoke(instance);
            } finally {
                clazz.getMethod("free").invoke(instance);
            }
        } catch (Exception e) {
            Ark.LOGGER.warn("Failed to extract sType for Vulkan struct '{}': {}", clazz.getSimpleName(), e.toString());
            return -1;
        }
    }

    // ── feature lookup ─────────────────────────────────────────────────

    /**
     * Scans all {@code boolean methodName()} getters whose uppercase constant exists in the class.
     */
    private static void registerStruct(Map<String, VulkanFeature> map, VulkanPNextStruct struct, Class<?> clazz) {
        for (var method : clazz.getDeclaredMethods()) {
            if (method.getReturnType() != boolean.class || method.getParameterCount() != 0) continue;
            if (!Modifier.isPublic(method.getModifiers())) continue;

            String name = method.getName();
            try {
                Field offsetField = clazz.getField(name.toUpperCase(Locale.ROOT));
                if (!Modifier.isStatic(offsetField.getModifiers()) || offsetField.getType() != int.class) continue;
                int offset = offsetField.getInt(null);
                map.putIfAbsent(name, new VulkanFeature(struct, name, offset));
            } catch (NoSuchFieldException | IllegalAccessException ignored) {
            }
        }
    }

    // ── pNext helpers ──────────────────────────────────────────────────

    /**
     * Checks extension compatibility against the given physical device.
     * <p>
     * Side effects:
     * <ul>
     *   <li>Each {@link Extension}'s unsupported-* lists are populated.</li>
     *   <li>{@code neededExtensions} receives every supported extension name.</li>
     *   <li>{@code neededFeatures} receives every supported {@link VulkanFeature}.</li>
     * </ul>
     */
    public static void check(
            VkPhysicalDevice device,
            VkPhysicalDeviceFeatures2 features,
            List<Extension> extensions,
            Set<String> neededExtensions,
            Set<VulkanFeature> neededFeatures
    ) {
        var supportedExtensions = listDeviceExtensions(device);

        // All pNext struct allocation and vkGetPhysicalDeviceFeatures2 must
        // happen within the same MemoryStack frame — otherwise the pNext
        // chain contains dangling pointers (use-after-free).
        try (var stack = MemoryStack.stackPush()) {
            ensureAllVersionStructsInChain(features, stack);
            VK11.vkGetPhysicalDeviceFeatures2(device, features);

            for (var extension : extensions) {
                var runtime = extension.getManifest().runtime;

                for (var ext : runtime.required_vulkan_extensions) {
                    if (supportedExtensions.contains(ext)) {
                        neededExtensions.add(ext);
                    } else {
                        extension.addUnsupportedRequiredVulkanExtension(ext);
                    }
                }

                for (var ext : runtime.optional_vulkan_extensions) {
                    if (supportedExtensions.contains(ext)) {
                        neededExtensions.add(ext);
                    } else {
                        extension.addUnsupportedOptionalVulkanExtension(ext);
                    }
                }

                for (var featureName : runtime.required_vulkan_features) {
                    var vf = lookupFeature(featureName);
                    if (vf == null) {
                        Ark.LOGGER.info("Required Vulkan feature '{}' from extension {} is unknown. The extension will be disabled.", featureName, extension.getManifest().id);
                        extension.addUnsupportedRequiredVulkanFeature(featureName);
                    } else if (vf.get(features)) {
                        neededFeatures.add(vf);
                    } else {
                        extension.addUnsupportedRequiredVulkanFeature(featureName);
                    }
                }

                for (var featureName : runtime.optional_vulkan_features) {
                    var vf = lookupFeature(featureName);
                    if (vf == null) {
                        Ark.LOGGER.info("Optional Vulkan feature '{}' from extension {} is unknown.", featureName, extension.getManifest().id);
                        extension.addUnsupportedOptionalVulkanFeature(featureName);
                    } else if (vf.get(features)) {
                        neededFeatures.add(vf);
                    } else {
                        extension.addUnsupportedOptionalVulkanFeature(featureName);
                    }
                }
            }
        }
    }

    // ── device extension enumeration ───────────────────────────────────

    /**
     * Looks up a VulkanFeature by name. Supports camelCase and snake_case variants.
     */
    @Nullable
    private static VulkanFeature lookupFeature(String name) {
        var vf = KNOWN_FEATURES.get(name);
        if (vf != null) return vf;
        return KNOWN_FEATURES.get(snakeToCamel(name));
    }

    // ── utilities ──────────────────────────────────────────────────────

    /**
     * Walks the pNext chain and attaches any missing feature structs.
     * An existing struct (matched by sType) is reused; a missing one is allocated
     * on the supplied {@code stack}.
     * <p>
     * The caller <b>must</b> keep {@code stack} alive until after
     * {@code vkGetPhysicalDeviceFeatures2} has been called.
     */
    private static void ensureAllVersionStructsInChain(VkPhysicalDeviceFeatures2 features, MemoryStack stack) {
        for (var descriptor : STRUCT_DESCRIPTORS.values()) {
            if (descriptor.sType() == VK10_STYPE) continue;
            descriptor.findOrCreateStructInPNextChain(features, stack);
        }
    }

    private static List<String> listDeviceExtensions(VkPhysicalDevice physicalDevice) {
        List<String> names = new ArrayList<>();
        try (var stack = MemoryStack.stackPush()) {
            var pCount = stack.callocInt(1);
            VK12.vkEnumerateDeviceExtensionProperties(physicalDevice, (String) null, pCount, null);
            int count = pCount.get(0);
            if (count > 0) {
                var buf = VkExtensionProperties.calloc(count, stack);
                VK12.vkEnumerateDeviceExtensionProperties(physicalDevice, (String) null, pCount, buf);
                for (int i = 0; i < buf.limit(); i++) {
                    names.add(buf.get(i).extensionNameString());
                }
            }
        }
        return names;
    }

    private static String snakeToCamel(String s) {
        var sb = new StringBuilder(s.length());
        var up = false;
        for (int i = 0; i < s.length(); i++) {
            var c = s.charAt(i);
            if (c == '_') {
                up = true;
            } else if (up) {
                sb.append(Character.toUpperCase(c));
                up = false;
            } else {
                sb.append(c);
            }
        }
        return sb.toString();
    }
}

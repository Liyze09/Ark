package io.github.liyze09.ark.extension;

import com.google.gson.Gson;
import com.google.gson.GsonBuilder;
import com.mojang.blaze3d.vulkan.init.VulkanFeature;
import io.github.liyze09.ark.Ark;
import net.fabricmc.loader.api.FabricLoader;
import org.jspecify.annotations.NonNull;
import org.jspecify.annotations.Nullable;
import org.lwjgl.vulkan.VkPhysicalDevice;
import org.lwjgl.vulkan.VkPhysicalDeviceFeatures2;

import java.io.File;
import java.io.FileInputStream;
import java.io.IOException;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Collections;
import java.util.HashSet;
import java.util.List;
import java.util.Set;
import java.util.stream.Collectors;
import java.util.zip.ZipInputStream;

public class ExtensionLoader {
    public static final Path extensionPath = FabricLoader.getInstance().getGameDir().resolve("arkextensions");
    private static final Gson GSON = new GsonBuilder()
            .registerTypeAdapter(ExtensionManifest.ValueOrList.class, new ExtensionManifest.ValueOrList.Deserializer())
            .create();

    static {
        var _ = extensionPath.toFile().mkdir();
    }

    private final Set<String> neededVulkanExtensions = new HashSet<>();
    private final Set<VulkanFeature> neededVulkanFeatures = new HashSet<>();
    private final Set<String> currentlyEnabledExtensions = new HashSet<>();
    private final Set<String> currentlyEnabledFeatures = new HashSet<>();
    private final List<Extension> extensions;

    // ── compatibility check ────────────────────────────────────────────
    private VkPhysicalDevice device;

    // ── runtime reload ───────────────────────────────────────────────
    private VkPhysicalDeviceFeatures2 deviceFeatures;

    public ExtensionLoader() {
        this.extensions = scanExtensions();
    }

    public static @NonNull List<Extension> scanExtensions() {
        var dir = extensionPath.toFile();
        var results = new ArrayList<Extension>();
        var files = dir.listFiles((f, name) -> name.endsWith(".zip"));
        if (files == null) {
            return results;
        }

        for (var file : files) {
            try {
                var manifest = readManifestFromZip(file);
                if (manifest != null) {
                    manifest.verify();
                    results.add(new Extension(file.getName(), manifest));
                }
            } catch (Exception e) {
                Ark.LOGGER.warn("Failed to read extension {}: {}", file.getName(), e.getMessage());
            }
        }

        return results;
    }

    // ── extension discovery ────────────────────────────────────────────

    private static @Nullable ExtensionManifest readManifestFromZip(File file) {
        try (var zin = new ZipInputStream(new FileInputStream(file))) {
            var entry = zin.getNextEntry();
            while (entry != null) {
                if (entry.getName().equals("manifest.json")) {
                    var json = new String(zin.readAllBytes());
                    return GSON.fromJson(json, ExtensionManifest.class);
                }
                entry = zin.getNextEntry();
            }
            Ark.LOGGER.warn("Failed to read zip {}: it doesn't have manifest.json.", file.getName());
        } catch (IOException e) {
            Ark.LOGGER.warn("Failed to read zip {}: {}", file.getName(), e.getMessage());
        }
        return null;
    }

    public void checkCompatibility(VkPhysicalDevice device, @NonNull VkPhysicalDeviceFeatures2 features) {
        this.device = device;
        this.deviceFeatures = features;
        neededVulkanExtensions.clear();
        neededVulkanFeatures.clear();
        CompatibilityChecker.check(device, features, extensions, neededVulkanExtensions, neededVulkanFeatures);
        currentlyEnabledExtensions.clear();
        currentlyEnabledExtensions.addAll(neededVulkanExtensions);
        currentlyEnabledFeatures.clear();
        for (var vf : neededVulkanFeatures) {
            currentlyEnabledFeatures.add(vf.name());
        }
    }

    /**
     * Re-scans the extension folder and adds newly discovered extensions.
     * If a new extension requires Vulkan extensions/features that are
     * hardware-supported but not currently enabled, its {@code needRestart}
     * field is set to {@code true}.
     */
    public void reload() {
        if (device == null || deviceFeatures == null) {
            Ark.LOGGER.warn("reload() called before checkCompatibility(); skipping");
            return;
        }
        var scanned = scanExtensions();
        var existingNames = extensions.stream()
                .map(Extension::getFileName)
                .collect(Collectors.toSet());

        for (var ext : scanned) {
            if (existingNames.contains(ext.getFileName())) continue;
            ext.getManifest().verify();

            var tempExts = new HashSet<String>();
            var tempFeats = new HashSet<VulkanFeature>();
            CompatibilityChecker.check(device, deviceFeatures, List.of(ext), tempExts, tempFeats);

            boolean needRestart = false;
            var runtime = ext.getManifest().runtime;
            for (var req : runtime.required_vulkan_extensions) {
                if (!ext.getUnsupportedRequiredVulkanExtensions().contains(req)
                        && !currentlyEnabledExtensions.contains(req)) {
                    needRestart = true;
                    break;
                }
            }
            for (var req : runtime.required_vulkan_features) {
                if (!ext.getUnsupportedRequiredVulkanFeatures().contains(req)
                        && !currentlyEnabledFeatures.contains(req)) {
                    needRestart = true;
                    break;
                }
            }
            ext.setNeedRestart(needRestart);

            neededVulkanExtensions.addAll(tempExts);
            neededVulkanFeatures.addAll(tempFeats);
            extensions.add(ext);
        }
    }

    public List<String> getNeededVulkanExtensions() {
        return neededVulkanExtensions.stream().toList();
    }

    public List<VulkanFeature> getNeededVulkanFeatures() {
        return neededVulkanFeatures.stream().toList();
    }

    public List<String> getCurrentlyEnabledExtensions() {
        return currentlyEnabledExtensions.stream().toList();
    }

    public List<String> getCurrentlyEnabledFeatures() {
        return currentlyEnabledFeatures.stream().toList();
    }

    public List<Extension> getExtensions() {
        return Collections.unmodifiableList(extensions);
    }
}

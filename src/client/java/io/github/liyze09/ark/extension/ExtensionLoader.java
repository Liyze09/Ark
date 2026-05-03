package io.github.liyze09.ark.extension;

import java.io.*;
import java.nio.file.Path;
import java.util.*;
import java.util.zip.ZipInputStream;

import com.google.gson.Gson;
import com.google.gson.GsonBuilder;
import com.mojang.blaze3d.vulkan.init.VulkanFeature;
import io.github.liyze09.ark.Ark;
import net.fabricmc.loader.api.FabricLoader;
import org.jspecify.annotations.NonNull;
import org.jspecify.annotations.Nullable;
import org.lwjgl.vulkan.VkPhysicalDevice;
import org.lwjgl.vulkan.VkPhysicalDeviceFeatures2;

public class ExtensionLoader {
    public List<Extension> extensions;
    private final Set<String> neededVulkanExtensions = new HashSet<>();
    private final Set<VulkanFeature> neededVulkanFeatures = new HashSet<>();

    public ExtensionLoader() {
        this.extensions = scanExtensions();
    }

    // ── compatibility check ────────────────────────────────────────────

    public void checkCompatibility(VkPhysicalDevice device, @NonNull VkPhysicalDeviceFeatures2 features) {
        neededVulkanExtensions.clear();
        neededVulkanFeatures.clear();
        CompatibilityChecker.check(device, features, extensions, neededVulkanExtensions, neededVulkanFeatures);
    }

    public Set<String> getNeededVulkanExtensions() {
        return Collections.unmodifiableSet(neededVulkanExtensions);
    }

    public Set<VulkanFeature> getNeededVulkanFeatures() {
        return Collections.unmodifiableSet(neededVulkanFeatures);
    }

    // ── extension discovery ────────────────────────────────────────────

    public static final Path extensionPath = FabricLoader.getInstance().getGameDir().resolve("arkextensions");

    static {
        var _ = extensionPath.toFile().mkdir();
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

    private static final Gson GSON = new GsonBuilder()
        .registerTypeAdapter(ExtensionManifest.ValueOrList.class, new ExtensionManifest.ValueOrList.Deserializer())
        .create();
}

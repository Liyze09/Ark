package io.github.liyze09.ark.extension;

import java.util.ArrayList;
import java.util.List;

public class Extension {
    public final List<String> unsupportedRequiredVulkanExtensions = new ArrayList<>(0);
    public final List<String> unsupportedRequiredVulkanFeatures = new ArrayList<>(0);
    public final List<String> unsupportedOptionalVulkanExtensions = new ArrayList<>(0);
    public final List<String> unsupportedOptionalVulkanFeatures = new ArrayList<>(0);
    private final ExtensionManifest manifest;
    private final String fileName;
    public boolean needRestart = false;

    public Extension(String fileName, ExtensionManifest manifest) {
        this.fileName = fileName;
        this.manifest = manifest;
    }

    public String getFileName() {
        return this.fileName;
    }

    public ExtensionManifest getManifest() {
        return this.manifest;
    }

    public boolean isAvailable() {
        return unsupportedRequiredVulkanExtensions.isEmpty() && unsupportedRequiredVulkanFeatures.isEmpty();
    }
}

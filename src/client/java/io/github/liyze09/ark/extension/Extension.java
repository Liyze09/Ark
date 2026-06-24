package io.github.liyze09.ark.extension;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

public class Extension {
    private final List<String> unsupportedRequiredVulkanExtensions = new ArrayList<>();
    private final List<String> unsupportedRequiredVulkanFeatures = new ArrayList<>();
    private final List<String> unsupportedOptionalVulkanExtensions = new ArrayList<>();
    private final List<String> unsupportedOptionalVulkanFeatures = new ArrayList<>();
    private final ExtensionManifest manifest;
    private final String fileName;
    private boolean needRestart = false;

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

    void addUnsupportedRequiredVulkanExtension(String ext) {
        unsupportedRequiredVulkanExtensions.add(ext);
    }

    void addUnsupportedRequiredVulkanFeature(String feature) {
        unsupportedRequiredVulkanFeatures.add(feature);
    }

    void addUnsupportedOptionalVulkanExtension(String ext) {
        unsupportedOptionalVulkanExtensions.add(ext);
    }

    void addUnsupportedOptionalVulkanFeature(String feature) {
        unsupportedOptionalVulkanFeatures.add(feature);
    }

    public List<String> getUnsupportedRequiredVulkanExtensions() {
        return Collections.unmodifiableList(unsupportedRequiredVulkanExtensions);
    }

    public List<String> getUnsupportedRequiredVulkanFeatures() {
        return Collections.unmodifiableList(unsupportedRequiredVulkanFeatures);
    }

    public List<String> getUnsupportedOptionalVulkanExtensions() {
        return Collections.unmodifiableList(unsupportedOptionalVulkanExtensions);
    }

    public List<String> getUnsupportedOptionalVulkanFeatures() {
        return Collections.unmodifiableList(unsupportedOptionalVulkanFeatures);
    }

    public boolean isNeedRestart() {
        return needRestart;
    }

    void setNeedRestart(boolean needRestart) {
        this.needRestart = needRestart;
    }
}

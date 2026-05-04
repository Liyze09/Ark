package io.github.liyze09.ark.extension;

import com.google.gson.JsonDeserializationContext;
import com.google.gson.JsonDeserializer;
import com.google.gson.JsonElement;
import org.jspecify.annotations.NonNull;

import java.lang.reflect.Type;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

@SuppressWarnings("unused")
public class ExtensionManifest {
    public String id;
    public String namespaces = "ark:unnamed";
    public String entrypoint = "script";
    public String entry_function = "initialize";
    public RuntimeArgs runtime = new RuntimeArgs();
    public String name;
    public String version = "";
    public String description = "";
    public String icon;
    public ValueOrList license = new ValueOrList("Unknown");
    public ValueOrList author = new ValueOrList("Me!");
    public ValueOrList contributors = new ValueOrList(List.of());
    public Map<String, String> contact = new HashMap<>(0);
    public Map<String, String> custom = new HashMap<>(0);

    public void verify() {
        if (id == null) {
            throw new IllegalArgumentException("Extension must have a 'id' field in manifest.json");
        } else {
            name = id;
        }
    }

    public static class RuntimeArgs {
        public String required_vulkan_version = "1.2.0";
        public String required_ark_version = "0.1.0";
        public List<String> required_vulkan_extensions = List.of();
        public List<String> optional_vulkan_extensions = List.of();
        public List<String> required_vulkan_features = List.of();
        public List<String> optional_vulkan_features = List.of();
        public List<String> optional_wasi_features = List.of();
    }

    public static class ValueOrList {
        public String value;
        public List<String> list;

        public ValueOrList(String value) {
            this.value = value;
            this.list = null;
        }

        public ValueOrList(List<String> list) {
            this.list = list;
            this.value = null;
        }

        public ValueOrList() {
        }

        public boolean isSingle() {
            return value != null;
        }

        public String asString() {
            return isSingle() ? value : String.join(", ", list);
        }

        public List<String> asList() {
            return isSingle() ? List.of(value) : list;
        }

        public static class Deserializer implements JsonDeserializer<ValueOrList> {
            @Override
            public ValueOrList deserialize(@NonNull JsonElement json, Type typeOfT, JsonDeserializationContext ctx) {
                var result = new ValueOrList();
                if (json.isJsonArray()) {
                    result.list = new ArrayList<>();
                    for (var el : json.getAsJsonArray()) {
                        result.list.add(el.getAsString());
                    }
                } else {
                    result.value = json.getAsString();
                }
                return result;
            }
        }
    }
}

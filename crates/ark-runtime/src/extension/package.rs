use std::{
    borrow::Cow,
    collections::HashMap,
    fmt::Display,
    io::{Cursor, Read},
};

use serde::Deserialize;

static MANIFEST_FILE: &str = "manifest.json";

pub fn parse_package(zip: &[u8]) -> anyhow::Result<ExtensionPackage> {
    let mut archive = zip::ZipArchive::new(Cursor::new(zip))?;

    let manifest = {
        let mut manifest_file = archive
            .by_name(MANIFEST_FILE)
            .map_err(|e| anyhow::anyhow!("Failed to find {}: {}", MANIFEST_FILE, e))?;
        let mut content = String::new();
        manifest_file.read_to_string(&mut content)?;
        serde_json::from_str(&content)?
    };

    let mut files = HashMap::new();
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)?;
        let name = file.name().to_string();
        if name == MANIFEST_FILE {
            continue;
        }
        let mut data = Vec::new();
        file.read_to_end(&mut data)?;
        files.insert(name, Cow::Owned(data));
    }

    Ok(ExtensionPackage { manifest, files })
}

#[derive(Debug)]
pub struct ExtensionPackage {
    pub manifest: ExtensionManifest,
    pub files: HashMap<String, Cow<'static, [u8]>>,
}

#[derive(Debug, Deserialize)]
pub struct ExtensionManifest {
    pub id: String,
    #[serde(default = "default_namespace")]
    pub namespaces: String,
    #[serde(default = "default_entrypoint")]
    pub entrypoint: String,
    #[serde(default)]
    pub runtime: RuntimeArgs,
    pub name: Option<String>,
    pub version: Option<String>,
    pub description: Option<String>,
    pub icon: Option<String>,
    pub license: Option<ValueOrList>,
    pub author: Option<ValueOrList>,
    pub contributors: Option<ValueOrList>,
    pub contact: Option<HashMap<String, String>>,
    pub custom: Option<HashMap<String, String>>,
}

fn default_namespace() -> String {
    "ark:unnamed".to_string()
}

fn default_entrypoint() -> String {
    "script".to_string()
}

#[derive(Debug, Deserialize)]
pub struct RuntimeArgs {
    #[serde(default = "default_required_vulkan_version")]
    pub required_vulkan_version: String,
    #[serde(default = "default_required_ark_version")]
    pub required_ark_version: String,
    #[serde(default)]
    pub required_vulkan_extensions: Vec<String>,
    #[serde(default)]
    pub optional_vulkan_extensions: Vec<String>,
    #[serde(default)]
    pub required_vulkan_features: Vec<String>,
    #[serde(default)]
    pub optional_vulkan_features: Vec<String>,
    #[serde(default)]
    pub optional_wasi_features: Vec<String>,
}

impl Default for RuntimeArgs {
    fn default() -> Self {
        Self {
            required_vulkan_version: default_required_vulkan_version(),
            required_ark_version: default_required_ark_version(),
            required_vulkan_extensions: vec![],
            optional_vulkan_extensions: vec![],
            required_vulkan_features: vec![],
            optional_vulkan_features: vec![],
            optional_wasi_features: vec![],
        }
    }
}

fn default_required_vulkan_version() -> String {
    "1.2.0".to_string()
}

fn default_required_ark_version() -> String {
    "0.1.0".to_string()
}

pub struct ExtensionIdentifier {
    pub id: String,
    pub namespace: String,
    pub version: Option<String>,
}

pub fn parse_vulkan_version(version: &str) -> anyhow::Result<vulkanalia::Version> {
    let mut parts: Vec<&str> = version.split('.').collect();
    if parts.len() == 2 {
        parts.push("0")
    }
    if parts.len() != 3 {
        return Err(anyhow::anyhow!("Invalid Vulkan version: {}", version));
    }
    Ok(vulkanalia::Version::new(
        parts[0].parse()?,
        parts[1].parse()?,
        parts[2].parse()?,
    ))
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum ValueOrList {
    Value(String),
    List(Vec<String>),
}

impl ValueOrList {
    pub fn as_string(&self) -> String {
        match self {
            ValueOrList::Value(v) => v.clone(),
            ValueOrList::List(l) => l.join(", "),
        }
    }
}

impl ExtensionIdentifier {
    pub fn new(id: &str, namespace: &str, version: Option<&str>) -> Self {
        Self {
            id: id.to_string(),
            namespace: namespace.to_string(),
            version: version.map(|v| v.to_string()),
        }
    }

    pub fn from_manifest(manifest: &ExtensionManifest) -> Self {
        Self::new(
            &manifest.id,
            &manifest.namespaces,
            manifest.version.as_deref(),
        )
    }
}

impl Display for ExtensionIdentifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(version) = &self.version {
            write!(f, "{}:{}@{}", self.namespace, self.id, version)
        } else {
            write!(f, "{}:{}", self.namespace, self.id)
        }
    }
}

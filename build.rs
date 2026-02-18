use std::fs::{create_dir_all};
use std::io::{stderr, stdout};
use std::path::Path;
use std::process::{exit, Command};

fn main() {
    let compilation_objects = ["mesh", "task"];

    for entry in walkdir::WalkDir::new("src/shaders") {
        let entry = entry.unwrap();
        let filename = entry.file_name().to_str().unwrap();
        if !filename.ends_with(".slang") {
            continue;
        }

        let shader_name = &filename.split_at(filename.find(".").unwrap()).0.to_owned();
        if !compilation_objects.contains(&shader_name.as_str()) {
            continue;
        }
        let mut out = Path::new("")
            .join("target")
            .join("shaders")
            .join(shader_name);
        out.set_extension("spv");
        create_dir_all(out.parent().unwrap()).unwrap();
        let stat = Command::new("slangc")
            .arg(entry.path())
            .arg("-target")
            .arg("spirv")
            .arg("-o")
            .arg(out)
            .stderr(stderr())
            .stdout(stdout())
            .status()
            .unwrap();
        if !stat.success() {
            exit(stat.code().unwrap());
        }
    }
}
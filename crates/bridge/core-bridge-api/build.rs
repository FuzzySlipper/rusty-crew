use std::{env, fs, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=bridge-manifest.toml");
    let manifest =
        fs::read_to_string("bridge-manifest.toml").expect("read canonical bridge-manifest.toml");
    let version = manifest_version(&manifest);
    let names = operation_names(&manifest);
    let mut generated =
        String::from("// @generated from bridge-manifest.toml by core-bridge-api/build.rs.\n");
    generated.push_str(&format!("pub const MANIFEST_VERSION: u32 = {version};\n"));
    generated.push_str("pub const OPERATION_NAMES: &[&str] = &[\n");
    for name in names {
        generated.push_str(&format!("    {name:?},\n"));
    }
    generated.push_str("];\n");

    let output =
        PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR")).join("bridge_operation_names.rs");
    fs::write(output, generated).expect("write generated bridge operation names");
}

fn manifest_version(manifest: &str) -> u32 {
    let mut in_manifest = false;
    for line in manifest.lines() {
        let line = line.trim();
        if line == "[manifest]" {
            in_manifest = true;
            continue;
        }
        if in_manifest && line.starts_with("version = ") {
            return line
                .split_once('=')
                .map(|(_, value)| value.trim())
                .expect("manifest version assignment")
                .parse()
                .expect("manifest version must be an unsigned integer");
        }
        if in_manifest && line.starts_with('[') {
            break;
        }
    }
    panic!("bridge manifest has no manifest version")
}

fn operation_names(manifest: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut in_operation = false;
    for line in manifest.lines() {
        let line = line.trim();
        if line == "[[operation]]" {
            in_operation = true;
            continue;
        }
        if in_operation && line.starts_with("name = ") {
            let value = line
                .split_once('=')
                .map(|(_, value)| value.trim())
                .and_then(|value| value.strip_prefix('"'))
                .and_then(|value| value.strip_suffix('"'))
                .expect("operation name must be a quoted string");
            assert!(!value.is_empty(), "operation name must not be empty");
            assert!(
                !names.iter().any(|name| name == value),
                "duplicate bridge operation {value}"
            );
            names.push(value.to_owned());
            in_operation = false;
        }
    }
    assert!(!names.is_empty(), "bridge manifest has no operations");
    names
}
